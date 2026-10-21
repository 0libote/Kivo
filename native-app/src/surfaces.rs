pub mod flow_bar {
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub enum Status {
        #[default]
        Hidden,
        Idle,
        Listening,
        Processing,
        Success,
        Error,
    }

    #[derive(Clone, Debug)]
    pub struct State {
        pub status: Status,
        pub transcript: String,
        pub message: Option<String>,
        pub level: f32,
        pub session: u64,
    }

    impl Default for State {
        fn default() -> Self {
            Self { status: Status::Idle, transcript: String::new(), message: None, level: 0.0, session: 0 }
        }
    }
}

pub mod writing_tools {
    use crate::ai::Action;

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub enum Mode {
        #[default]
        Menu,
        Processing,
        Result,
        Error,
        Chat,
    }

    #[derive(Clone, Debug, Default)]
    pub struct State {
        pub mode: Mode,
        pub input: String,
        pub custom_instruction: String,
        pub result: String,
        pub error: Option<String>,
        pub action: Option<Action>,
        pub can_replace: bool,
    }
}

pub mod settings_surface {
    #[derive(Clone, Debug, Default)]
    pub struct State {
        pub api_key: String,
        pub status: Option<String>,
    }
}

pub mod onboarding {
    #[derive(Clone, Debug, Default)]
    pub struct State {
        pub step: usize,
    }
    pub const STEPS: [&str; 4] = ["Welcome", "Permissions", "Shortcuts", "AI key"];
}
