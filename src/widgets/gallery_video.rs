// SPDX-License-Identifier: GPL-3.0-or-later
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib};

use crate::widgets::gallery_item::GalleryItemImpl;

use super::video_player;

mod imp {
    use std::cell::OnceCell;

    use super::*;

    #[derive(Debug, Default)]
    pub struct GalleryVideo {
        pub video_player: OnceCell<video_player::VideoPlayer>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GalleryVideo {
        const NAME: &'static str = "GalleryVideo";
        type Type = super::GalleryVideo;
        type ParentType = crate::GalleryItem;
    }

    impl ObjectImpl for GalleryVideo {}

    impl WidgetImpl for GalleryVideo {
        fn unmap(&self) {
            self.obj().pause();
            self.parent_unmap();
        }
    }

    impl BinImpl for GalleryVideo {}
    impl GalleryItemImpl for GalleryVideo {}
}

glib::wrapper! {
    pub struct GalleryVideo(ObjectSubclass<imp::GalleryVideo>)
        @extends gtk::Widget, adw::Bin, crate::GalleryItem;
}

impl GalleryVideo {
    pub fn new(file: &gio::File, load: bool) -> Self {
        glib::Object::builder()
            .property("load", load)
            .property("file", file)
            .property("is-picture", false)
            .build()
    }

    pub fn stream(&self) -> anyhow::Result<&gtk::MediaStream> {
        if let Some(video_player) = self.imp().video_player.get() {
            Ok(video_player.stream())
        } else {
            Err(anyhow::anyhow!(
                "Tried to stream before video player loaded"
            ))
        }
    }

    pub async fn load_texture(&self) -> anyhow::Result<()> {
        let imp = self.imp();

        self.set_started_loading(true);

        let file = self.file();

        let video_player = imp
            .video_player
            .get_or_init(|| crate::VideoPlayer::default());

        video_player.set_file(&file);

        self.upcast_ref::<crate::GalleryItem>()
            .set_item(video_player.upcast_ref());

        if let Some(texture) = video_player.thumbnail().await {
            self.set_thumbnail(texture);
        }

        Ok(())
    }

    pub fn pause(&self) {
        if let Some(video_player) = self.imp().video_player.get() {
            video_player.pause()
        } else {
            log::warn!("Tried to pause before video player loaded")
        }
    }

    // Ugh
    fn file(&self) -> gio::File {
        self.upcast_ref::<crate::GalleryItem>().file()
    }

    fn set_started_loading(&self, value: bool) {
        self.upcast_ref::<crate::GalleryItem>()
            .set_started_loading(value);
    }

    fn set_thumbnail(&self, value: &gdk::Texture) {
        self.upcast_ref::<crate::GalleryItem>().set_thumbnail(value);
    }
}
