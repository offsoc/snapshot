// SPDX-License-Identifier: GPL-3.0-or-later
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib};

use crate::widgets::gallery_item::imp::GalleryItemPropertiesExt;
use crate::widgets::gallery_item::GalleryItemImpl;

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct GalleryPicture {
        pub picture: gtk::Picture,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GalleryPicture {
        const NAME: &'static str = "GalleryPicture";
        type Type = super::GalleryPicture;
        type ParentType = crate::GalleryItem;
    }

    impl ObjectImpl for GalleryPicture {
        fn constructed(&self) {
            self.parent_constructed();

            let widget = self.obj();

            widget
                .upcast_ref::<crate::GalleryItem>()
                .set_item(self.picture.upcast_ref());

            if let Some(basename) = widget.file().basename() {
                let label = basename.display().to_string();
                self.picture
                    .update_property(&[gtk::accessible::Property::Label(&label)]);
            }
        }
    }
    impl WidgetImpl for GalleryPicture {}
    impl BinImpl for GalleryPicture {}
    impl GalleryItemImpl for GalleryPicture {}
}

glib::wrapper! {
    pub struct GalleryPicture(ObjectSubclass<imp::GalleryPicture>)
        @extends gtk::Widget, adw::Bin, crate::GalleryItem;
}

impl GalleryPicture {
    /// Creates a new picture for the gallery. The texture will be load at
    /// construct only if `load` is set to `true`, otherwise it will be load
    /// when we want to snapshot it.
    pub fn new(file: &gio::File, load: bool) -> Self {
        glib::Object::builder()
            .property("load", load)
            .property("file", file)
            .property("is-picture", true)
            .build()
    }

    pub async fn load_texture(&self) -> anyhow::Result<()> {
        let imp = self.imp();

        GalleryItemPropertiesExt::set_started_loading(self, true);

        let file = GalleryItemPropertiesExt::file(self);
        let (sender, receiver) = futures_channel::oneshot::channel();

        let _ = std::thread::Builder::new()
            .name("Load Texture".to_string())
            .spawn(move || {
                let result = gdk::Texture::from_file(&file);
                let _ = sender.send(result);
            });

        let texture = receiver.await.unwrap()?;

        imp.picture.set_paintable(Some(&texture));
        self.set_thumbnail(&texture);

        Ok(())
    }
}
