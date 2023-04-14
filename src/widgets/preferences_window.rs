use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::CompositeTemplate;
use gtk::{gio, glib};

use crate::config;

mod imp {
    use super::*;

    use once_cell::sync::OnceCell;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/org/gnome/Snapshot/ui/preferences_window.ui")]
    pub struct PreferencesWindow {
        settings: OnceCell<gio::Settings>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PreferencesWindow {
        const NAME: &'static str = "PreferencesWindow";
        type Type = super::PreferencesWindow;
        type ParentType = adw::PreferencesWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[gtk::template_callbacks]
    impl PreferencesWindow {
        #[template_callback]
        fn on_combo_selected_notify(&self, _pspec: glib::ParamSpec, combo_row: &adw::ComboRow) {
            self.settings
                .get()
                .unwrap()
                .set_enum("picture-format", combo_row.selected() as i32)
                .unwrap();
        }
    }

    impl ObjectImpl for PreferencesWindow {
        fn constructed(&self) {
            self.parent_constructed();

            let settings = gio::Settings::new(config::APP_ID);

            let action_group = gio::SimpleActionGroup::new();

            let play_shutter_sound = settings.create_action("play-shutter-sound");
            action_group.add_action(&play_shutter_sound);

            self.obj()
                .insert_action_group("preferences-window", Some(&action_group));

            self.settings.set(settings).unwrap();
        }
    }

    impl WidgetImpl for PreferencesWindow {}
    impl WindowImpl for PreferencesWindow {}
    impl AdwWindowImpl for PreferencesWindow {}
    impl PreferencesWindowImpl for PreferencesWindow {}
}

glib::wrapper! {
    pub struct PreferencesWindow(ObjectSubclass<imp::PreferencesWindow>)
        @extends gtk::Widget, gtk::Window, adw::Window, adw::PreferencesWindow;
}

impl PreferencesWindow {
    pub fn new(window: &crate::Window) -> Self {
        glib::Object::builder()
            .property("transient-for", window)
            .build()
    }
}
