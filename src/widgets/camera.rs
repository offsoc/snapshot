// SPDX-License-Identifier: GPL-3.0-or-later
use ashpd::desktop::camera;
use gettextrs::gettext;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};
use gtk::{prelude::*, CompositeTemplate};
use std::os::unix::io::RawFd;

use crate::CameraRow;
use crate::{config, utils};

const PROVIDER_TIMEOUT: u64 = 2;

mod imp {
    use super::*;

    use gtk::glib::Properties;
    use once_cell::unsync::OnceCell;
    use std::cell::{Cell, RefCell};

    #[derive(Debug, Default, CompositeTemplate, Properties)]
    #[properties(wrapper_type = super::Camera)]
    #[template(resource = "/org/gnome/Snapshot/ui/camera.ui")]
    pub struct Camera {
        pub stream_list: RefCell<gio::ListStore>,
        pub selection: gtk::SingleSelection,
        pub provider: OnceCell<aperture::DeviceProvider>,
        pub players: RefCell<Option<gtk::MediaFile>>,
        settings: OnceCell<gio::Settings>,

        #[property(get, set = Self::set_breakpoint, explicit_notify, builder(crate::Breakpoint::default()))]
        pub breakpoint: Cell<crate::Breakpoint>,

        #[template_child]
        pub gallery_button: TemplateChild<crate::GalleryButton>,
        #[template_child]
        pub camera_menu_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub camera_switch_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub camera_menu_button_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub viewfinder: TemplateChild<aperture::Viewfinder>,
        #[template_child]
        pub flash_bin: TemplateChild<crate::FlashBin>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub spinner: TemplateChild<gtk::Spinner>,
        #[template_child]
        pub shutter_button: TemplateChild<crate::ShutterButton>,

        #[template_child]
        pub camera_controls: TemplateChild<gtk::Box>,
        #[template_child]
        pub countdown_button: TemplateChild<gtk::MenuButton>,

        #[template_child]
        pub sidebar_horizontal_start: TemplateChild<gtk::CenterBox>,
        #[template_child]
        pub sidebar_horizontal_end: TemplateChild<gtk::CenterBox>,
        #[template_child]
        pub sidebar_vertical_start: TemplateChild<gtk::CenterBox>,
        #[template_child]
        pub sidebar_vertical_end: TemplateChild<gtk::CenterBox>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Camera {
        const NAME: &'static str = "Camera";
        type Type = super::Camera;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_callbacks();
            klass.set_layout_manager_type::<gtk::BinLayout>();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[gtk::template_callbacks]
    impl Camera {
        pub fn settings(&self) -> &gio::Settings {
            self.settings
                .get_or_init(|| gio::Settings::new(config::APP_ID))
        }

        fn set_breakpoint(&self, value: crate::Breakpoint) {
            if value == self.breakpoint.replace(value) {
                return;
            }

            self.sidebar_horizontal_start.set_visible(false);
            self.sidebar_vertical_start.set_visible(false);

            self.sidebar_horizontal_end.set_visible(false);
            self.sidebar_horizontal_end
                .set_start_widget(gtk::Widget::NONE);
            self.sidebar_horizontal_end
                .set_center_widget(gtk::Widget::NONE);

            self.sidebar_vertical_end.set_visible(false);
            self.sidebar_vertical_end
                .set_center_widget(gtk::Widget::NONE);
            self.sidebar_vertical_end.set_end_widget(gtk::Widget::NONE);

            match value {
                crate::Breakpoint::SingleVertical => {
                    self.camera_controls
                        .set_orientation(gtk::Orientation::Vertical);

                    self.sidebar_vertical_end.set_visible(true);
                    self.sidebar_vertical_end
                        .start_widget()
                        .iter()
                        .for_each(|widget| widget.set_visible(true));
                    self.sidebar_vertical_end
                        .set_center_widget(Some(&self.camera_controls.get()));
                    self.sidebar_vertical_end
                        .set_end_widget(Some(&self.countdown_button.get()));
                }
                crate::Breakpoint::DualVertical => {
                    self.camera_controls
                        .set_orientation(gtk::Orientation::Vertical);

                    self.sidebar_vertical_start.set_visible(true);
                    self.sidebar_vertical_end.set_visible(true);
                    self.sidebar_vertical_end
                        .start_widget()
                        .iter()
                        .for_each(|widget| widget.set_visible(false));
                    self.sidebar_vertical_end
                        .set_center_widget(Some(&self.camera_controls.get()));
                }
                crate::Breakpoint::SingleHorizontal => {
                    self.camera_controls
                        .set_orientation(gtk::Orientation::Horizontal);

                    self.sidebar_horizontal_end.set_visible(true);
                    self.sidebar_horizontal_end
                        .set_start_widget(Some(&self.countdown_button.get()));
                    self.sidebar_horizontal_end
                        .set_center_widget(Some(&self.camera_controls.get()));
                    self.sidebar_horizontal_end
                        .end_widget()
                        .iter()
                        .for_each(|widget| widget.set_visible(true));
                }
                crate::Breakpoint::DualHorizontal => {
                    self.camera_controls
                        .set_orientation(gtk::Orientation::Horizontal);

                    self.sidebar_horizontal_start.set_visible(true);
                    self.sidebar_horizontal_end.set_visible(true);
                    self.sidebar_horizontal_end
                        .set_center_widget(Some(&self.camera_controls.get()));
                    self.sidebar_horizontal_end
                        .end_widget()
                        .iter()
                        .for_each(|widget| widget.set_visible(false));
                }
            }

            self.obj().notify_breakpoint();
        }

        #[template_callback]
        fn on_camera_switch_button_clicked(&self) {
            let provider = self.provider.get().unwrap();

            let current = self.viewfinder.camera();

            let mut pos = 0;
            if current == provider.camera(0) {
                pos += 1;
            };
            if let Some(camera) = provider.camera(pos) {
                self.viewfinder.set_camera(Some(camera));
            }
        }
    }

    impl ObjectImpl for Camera {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            let popover = gtk::Popover::new();
            popover.add_css_class("menu");

            let provider = aperture::DeviceProvider::instance();
            self.provider.set(provider.clone()).unwrap();
            provider.connect_items_changed(glib::clone!(@weak obj => move |provider, _, _, _| {
                obj.update_cameras(provider);
            }));
            obj.update_cameras(provider);

            self.selection.set_model(Some(provider));
            let factory = gtk::SignalListItemFactory::new();
            factory.connect_setup(|_, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();
                let camera_row = CameraRow::default();

                item.set_child(Some(&camera_row));
            });
            let selection = &self.selection;
            factory.connect_bind(glib::clone!(@weak selection => move |_, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();
                let child = item.child().unwrap();
                let row = child.downcast_ref::<CameraRow>().unwrap();

                let item = item.item().and_downcast::<aperture::Camera>().unwrap();
                row.set_item(&item);

                selection.connect_selected_item_notify(glib::clone!(@weak row, @weak item => move |selection| {
                    if let Some(selected_item) = selection.selected_item() {
                        row.set_selected(selected_item == item);
                    } else {
                        row.set_selected(false);
                    }
                }));
            }));
            let list_view = gtk::ListView::new(Some(self.selection.clone()), Some(factory));

            popover.set_child(Some(&list_view));

            self.selection.connect_selected_item_notify(
                glib::clone!(@weak obj, @weak popover => move |selection| {
                    if let Some(selected_item) = selection.selected_item() {
                        let camera = selected_item.downcast::<aperture::Camera>().ok();

                        if matches!(obj.imp().viewfinder.state(), aperture::ViewfinderState::Ready | aperture::ViewfinderState::Error) {
                            obj.imp().viewfinder.set_camera(camera);
                        }
                    }
                    popover.popdown();
                }),
            );

            self.camera_menu_button.set_popover(Some(&popover));

            // This spinner stops running when the device provider finds any
            // camera device.
            self.spinner.start();
            self.stack.set_visible_child_name("loading");
        }

        fn dispose(&self) {
            self.dispose_template();
        }

        fn properties() -> &'static [glib::ParamSpec] {
            Self::derived_properties()
        }

        fn property(&self, id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            Self::derived_property(self, id, pspec)
        }

        fn set_property(&self, id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            Self::derived_set_property(self, id, value, pspec)
        }
    }

    impl WidgetImpl for Camera {}
}

glib::wrapper! {
    pub struct Camera(ObjectSubclass<imp::Camera>)
        @extends gtk::Widget;
}

impl Default for Camera {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Camera {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn start(&self) {
        let provider = self.imp().provider.get().unwrap();

        let ctx = glib::MainContext::default();
        ctx.spawn_local(
            glib::clone!(@weak self as obj, @strong provider => async move {
                match stream().await {
                    Ok(fd) => {
                        if let Err(err) = provider.set_fd(fd) {
                            log::error!("Could not use the camera portal: {err}");
                        };
                    }
                    Err(err) => {
                        log::warn!("Could not use the camera portal: {err}");
                    }
                }
                if let Err(err) = provider.start() {
                    log::error!("Could not start the device provider: {err}");
                } else {
                    log::debug!("Device provider started");
                };
            }),
        );

        // FIXME Set to not-found on ViewfinderState::Error;

        // FIXME This is super arbitrary
        let duration = std::time::Duration::from_secs(PROVIDER_TIMEOUT);
        glib::timeout_add_local_once(
            duration,
            glib::clone!(@weak self as obj => move || {
                let imp = obj.imp();
                if imp.stack.visible_child_name().as_deref() == Some("loading") {
                    imp.spinner.stop();
                    imp.stack.set_visible_child_name("not-found");
                }
            }),
        );
    }

    pub async fn start_recording(&self, format: crate::VideoFormat) -> anyhow::Result<()> {
        let filename = utils::video_file_name(format);
        let path = utils::videos_dir()?.join(filename);

        self.imp().viewfinder.start_recording(path)?;

        Ok(())
    }

    pub fn stop_recording(&self) {
        let imp = self.imp();
        if matches!(imp.viewfinder.state(), aperture::ViewfinderState::Ready)
            && imp.viewfinder.is_recording()
        {
            if let Err(err) = imp.viewfinder.stop_recording() {
                log::error!("Could not stop camera: {err}");
            };
        }
    }

    pub async fn take_picture(&self, format: crate::PictureFormat) -> anyhow::Result<()> {
        let imp = self.imp();
        let window = self.root().and_downcast::<crate::Window>().unwrap();

        // We enable the shutter whenever picture-stored is emited.
        window.set_shutter_enabled(false);

        let filename = utils::picture_file_name(format);
        let path = utils::pictures_dir()?.join(filename);

        imp.viewfinder.take_picture(path)?;
        imp.flash_bin.flash();

        let settings = imp.settings();
        if settings.boolean("play-shutter-sound") {
            self.play_shutter_sound();
        }

        Ok(())
    }

    fn play_shutter_sound(&self) {
        // If we don't hold a reference to it there is a condition race which
        // will cause the sound to play only sometimes.
        let resource = "/org/gnome/Snapshot/sounds/camera-shutter.wav";
        let player = gtk::MediaFile::for_resource(resource);
        player.play();

        self.imp().players.replace(Some(player));
    }

    pub fn set_countdown(&self, countdown: u32) {
        self.imp().shutter_button.set_countdown(countdown);
    }

    pub fn start_countdown(&self) {
        self.imp().shutter_button.start_countdown();
    }

    pub fn stop_countdown(&self) {
        self.imp().shutter_button.stop_countdown();
    }

    pub fn shutter_mode(&self) -> crate::ShutterMode {
        self.imp().shutter_button.shutter_mode()
    }

    pub fn set_shutter_mode(&self, shutter_mode: crate::ShutterMode) {
        if matches!(shutter_mode, crate::ShutterMode::Picture) {
            self.stop_recording();
        }
        self.imp().shutter_button.set_shutter_mode(shutter_mode);
    }

    pub fn set_gallery(&self, gallery: crate::Gallery) {
        let imp = self.imp();

        imp.viewfinder.connect_picture_done(
            glib::clone!(@weak gallery, @weak self as obj => move |_, file| {
                let window = obj.root().and_downcast::<crate::Window>().unwrap();
                window.set_shutter_enabled(true);
                // TODO Maybe report error via toast on None
                if let Some(file) = file {
                    gallery.add_image(file);
                }
            }),
        );
        imp.viewfinder.connect_recording_done(
            glib::clone!(@weak gallery, @weak self as obj => move |_, file| {
                let imp = obj.imp();
                // TODO Maybe report error via toast on None
                if let Some(file) = file {
                    gallery.add_video(file);
                }
                if matches!(imp.shutter_button.shutter_mode(), crate::ShutterMode::Recording) {
                    imp.shutter_button.set_shutter_mode(crate::ShutterMode::Video);
                }
            }),
        );
        imp.viewfinder.connect_state_notify(|viewfinder| {
            if matches!(viewfinder.state(), aperture::ViewfinderState::Error) {
                let window = viewfinder.root().and_downcast::<crate::Window>().unwrap();
                window.send_toast(&gettext("Could not play camera stream"));
            }
        });
        imp.gallery_button.set_gallery(&gallery);
    }

    fn update_cameras(&self, provider: &aperture::DeviceProvider) {
        let imp = self.imp();
        imp.spinner.stop();

        let n_cameras = provider.n_items();
        if n_cameras == 0 {
            imp.stack.set_visible_child_name("not-found");
        } else {
            imp.stack.set_visible_child_name("camera");
        }

        // NOTE We have a stack with an empty bin so that hiding the button does
        // not ruin the layout.
        match n_cameras {
            0 | 1 => imp
                .camera_menu_button_stack
                .set_visible_child_name("fake-widget"),
            2 => imp
                .camera_menu_button_stack
                .set_visible_child(&imp.camera_switch_button.get()),
            _ => imp
                .camera_menu_button_stack
                .set_visible_child(&imp.camera_menu_button.get()),
        }
    }
}

async fn stream() -> ashpd::Result<RawFd> {
    let proxy = camera::Camera::new().await?;
    proxy.request_access().await?;

    proxy.open_pipe_wire_remote().await
}
