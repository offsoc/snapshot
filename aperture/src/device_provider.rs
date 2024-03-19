// SPDX-License-Identifier: GPL-3.0-or-later
use std::cell::RefCell;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::io::RawFd;
use std::sync::Once;

use gst::prelude::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};

use crate::utils;

static STARTED: Once = Once::new();

mod imp {
    use std::cell::OnceCell;

    use glib::Properties;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Default, Properties)]
    #[properties(wrapper_type = super::DeviceProvider)]
    pub struct DeviceProvider {
        pub inner: OnceCell<gst::DeviceProvider>,
        pub cameras: RefCell<Vec<crate::Camera>>,
        pub bus_watch: OnceCell<gst::bus::BusWatchGuard>,

        pub fd: RefCell<Option<OwnedFd>>,

        pub default_cb: OnceCell<Box<dyn Fn(&crate::Camera) -> bool + 'static>>,

        #[property(get = Self::started)]
        pub started: std::marker::PhantomData<bool>,
    }

    impl DeviceProvider {
        pub fn append(&self, camera: crate::Camera) {
            let pos = self.cameras.borrow().len() as u32;
            self.cameras.borrow_mut().push(camera.clone());
            self.obj().items_changed(pos, 0, 1);
            self.obj().emit_camera_added(&camera);
        }

        fn started(&self) -> bool {
            STARTED.is_completed()
        }

        pub fn has_camera(&self, camera: &crate::Camera) -> bool {
            self.cameras
                .borrow()
                .iter()
                .any(|c| c.device() == camera.device())
        }

        pub fn remove(&self, device: crate::Camera) {
            let guard = self.cameras.borrow();
            let Some((pos, _)) = guard
                .iter()
                .enumerate()
                .find(|(_idx, x)| x.target_object() == device.target_object())
            else {
                log::error!(
                    "Tried to remove camera with target-object {:?} but it wasn't in the vec?",
                    device.target_object()
                );
                return;
            };
            drop(guard);
            self.cameras.borrow_mut().remove(pos);
            self.obj().items_changed(pos as u32, 1, 0);
            self.obj().emit_camera_removed(&device);
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DeviceProvider {
        const NAME: &'static str = "ApertureDeviceProvider";
        type Type = super::DeviceProvider;
        type Interfaces = (gio::ListModel,);
    }

    impl ListModelImpl for DeviceProvider {
        fn item_type(&self) -> glib::Type {
            crate::Camera::static_type()
        }
        fn n_items(&self) -> u32 {
            self.cameras.borrow().len() as u32
        }
        fn item(&self, position: u32) -> Option<glib::Object> {
            self.cameras
                .borrow()
                .get(position as usize)
                .cloned()
                .and_upcast()
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for DeviceProvider {
        fn constructed(&self) {
            self.parent_constructed();

            crate::ensure_init();

            if let Some(provider) = gst::DeviceProviderFactory::by_name("pipewiredeviceprovider") {
                self.inner.set(provider).unwrap();
            }
        }

        fn dispose(&self) {
            let inner = self.inner.get().unwrap();
            if inner.is_started() {
                inner.stop();
            }
            inner.set_property("fd", -1);
        }

        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: Lazy<Vec<glib::subclass::Signal>> = Lazy::new(|| {
                vec![
                    glib::subclass::Signal::builder("camera-added")
                        .param_types([crate::Camera::static_type()])
                        .build(),
                    glib::subclass::Signal::builder("camera-removed")
                        .param_types([crate::Camera::static_type()])
                        .build(),
                ]
            });
            SIGNALS.as_ref()
        }
    }
}

glib::wrapper! {
    /// A provider for available camera devices.
    ///
    /// It is used to find and monitor cameras that can be used in Aperture. It also handles the
    /// creation of [`Camera`][crate::Camera] objects.
    ///
    /// ## Signals
    ///
    ///
    /// #### `camera-added`
    ///  This signal is emitted after a camera has been added to the device provider.
    ///
    ///
    /// #### `camera-removed`
    ///  This signal is emitted after a camera has been removed from the device provider.
    ///
    /// # Implements
    ///
    /// [`gio::prelude::ListModelExt`][trait@gtk::gio::prelude::ListModelExt], [`glib::ObjectExt`][trait@gtk::glib::ObjectExt]
    pub struct DeviceProvider(ObjectSubclass<imp::DeviceProvider>)
        @implements gio::ListModel;
}

impl DeviceProvider {
    /// Gets the default [`DeviceProvider`][crate::DeviceProvider].
    pub fn instance() -> &'static Self {
        use glib::thread_guard::ThreadGuard;
        use once_cell::sync::Lazy;

        struct Wrapper(ThreadGuard<crate::DeviceProvider>);
        // SAFETY: We only ever hand out a reference to the contained object on the one
        // thread it was created one so no two threads can access it at the same
        // time.
        unsafe impl Sync for Wrapper {}

        static SINGLETON: Lazy<Wrapper> = Lazy::new(|| {
            Wrapper(ThreadGuard::new(
                glib::Object::new::<crate::DeviceProvider>(),
            ))
        });

        SINGLETON.0.get_ref()
    }

    /// Starts the device provider
    ///
    /// Like [`start`] but allows allows to specify a criteria for selecting a
    /// default camera.
    ///
    /// This will be taken into account when the [`Viewfinder`] has to choose a
    /// default camera.
    pub fn start_with_default<F: Fn(&crate::Camera) -> bool + 'static>(
        &self,
        f: F,
    ) -> Result<(), crate::ProviderError> {
        if STARTED.is_completed() {
            return Ok(());
        } else {
            STARTED.call_once(|| ());
        }

        let imp = self.imp();

        let Some(provider) = self.imp().inner.get() else {
            return Err(crate::ProviderError::MissingPlugin(
                "pipewiredeviceprovider",
            ));
        };
        provider.start()?;

        let cameras = provider
            .devices()
            .iter()
            .filter(|d| d.device_class() == "Video/Source")
            .map(crate::Camera::new)
            .collect::<Vec<_>>();
        let n_items = cameras.len() as u32;
        cameras.iter().for_each(|camera| {
            log::debug!(
                "Camera found: {}, target-object: {:?}\nProperties {:#?}",
                camera.display_name(),
                camera.target_object(),
                camera.properties(),
            );
        });
        self.imp().cameras.replace(cameras);
        self.items_changed(0, 0, n_items);

        let bus = provider.bus();
        let watch = bus
            .add_watch_local(
                glib::clone!(@weak self as obj => @default-return glib::ControlFlow::Break,
                move |_, msg| {
                    obj.handle_message(msg);
                    glib::ControlFlow::Continue
                }),
            )
            .expect("Failed to add bus watch");
        imp.bus_watch.set(watch).unwrap();

        let _ = imp.default_cb.set(Box::new(f));

        self.notify_started();

        Ok(())
    }

    /// Starts the device provider represented by `self`.
    ///
    /// This function is idempotent when there are no errors.
    pub fn start(&self) -> Result<(), crate::ProviderError> {
        self.start_with_default(|_| false)
    }

    pub(crate) fn default_camera(&self) -> Option<crate::Camera> {
        let imp = self.imp();
        let cameras = imp.cameras.borrow();
        imp.default_cb
            .get()
            .and_then(|f| cameras.iter().find(|c| f(c)))
            .cloned()
    }

    /// Gets a [`Camera`] object for the given camera index.
    ///
    /// # Returns
    ///
    /// a [`Camera`] at `position`.
    ///
    /// [`Camera`]: crate::Camera
    pub fn camera(&self, position: u32) -> Option<crate::Camera> {
        self.item(position).and_downcast()
    }

    /// Set a valid file description to load and monitor cameras from.
    ///
    /// This file descriptor should point to a valid Pipewire remote where
    /// camera nodes are available. This provider should only provide camera
    /// nodes.
    ///
    /// One way to get a valid descriptor is with the [`org.freedesktop.portal.Camera`](https://flatpak.github.io/xdg-desktop-portal/#gdbus-org.freedesktop.portal.Camera)
    /// XDG portal, using the `OpenPipeWireRemote()` method.
    pub fn set_fd(&self, fd: OwnedFd) -> Result<(), crate::PipewireError> {
        if STARTED.is_completed() {
            return Err(crate::PipewireError::ProvidedStarted);
        }
        let raw_fd = fd.as_raw_fd();
        let provider = self.imp().inner.get().unwrap();
        log::debug!("Starting device provider with file descriptor: {raw_fd}");
        if provider.has_property("fd", Some(RawFd::static_type())) {
            provider.set_property("fd", raw_fd);
            if let Some(old_fd) = self.imp().fd.replace(Some(fd)) {
                log::debug!("Freeing fd {}", old_fd.as_raw_fd());
            }

            Ok(())
        } else {
            log::warn!("Pipewire device provider does not have the `fd` property, please update to a version newer than 0.3.64");
            Err(crate::PipewireError::OldVersion)
        }
    }

    pub fn connect_camera_added<F: Fn(&Self, &crate::Camera) + 'static>(&self, f: F) {
        self.connect_closure(
            "camera-added",
            false,
            glib::closure_local!(|obj, camera| {
                f(obj, camera);
            }),
        );
    }

    pub fn connect_camera_removed<F: Fn(&Self, &crate::Camera) + 'static>(&self, f: F) {
        self.connect_closure(
            "camera-removed",
            false,
            glib::closure_local!(|obj, camera| {
                f(obj, camera);
            }),
        );
    }

    fn emit_camera_added(&self, camera: &crate::Camera) {
        self.emit_by_name::<()>("camera-added", &[&camera]);
    }

    fn emit_camera_removed(&self, camera: &crate::Camera) {
        self.emit_by_name::<()>("camera-removed", &[&camera]);
    }

    fn handle_message(&self, msg: &gst::Message) {
        let imp = self.imp();
        match msg.view() {
            gst::MessageView::Error(err) => {
                log::error!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );
            }
            gst::MessageView::DeviceAdded(e) => {
                if let Some(s) = e.structure() {
                    if let Ok(device) = s.get::<gst::Device>("device") {
                        if "Video/Source" == device.device_class() {
                            let device = crate::Camera::new(&device);
                            if !imp.has_camera(&device) {
                                // We ignore/filter IR cameras.
                                if device.caps().as_ref().is_some_and(utils::caps::is_infrared) {
                                    return;
                                }
                                log::debug!(
                                    "Camera added: {}, target-object: {:?}\nProperties {:#?}",
                                    device.display_name(),
                                    device.target_object(),
                                    device.properties(),
                                );
                                imp.append(device);
                            }
                        };
                    }
                }
            }
            gst::MessageView::DeviceRemoved(e) => {
                if let Some(s) = e.structure() {
                    if let Ok(device) = s.get::<gst::Device>("device") {
                        if "Video/Source" == device.device_class() {
                            let n_items = self.n_items();
                            for n in 0..n_items {
                                if let Some(nth_device) = self.camera(n) {
                                    if device == nth_device.device() {
                                        self.imp().remove(nth_device);
                                        log::debug!("Camera removed: {}", device.display_name());
                                        break;
                                    }
                                };
                            }
                        };
                    }
                }
            }
            _ => (),
        }
    }
}
