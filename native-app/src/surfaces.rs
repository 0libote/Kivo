pub mod flow_bar {
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub enum Status {
        #[default]
        Hidden,
        Idle,
        Listening,
        Success,
        Error,
    }

    #[derive(Clone, Debug)]
    pub struct State {
        pub status: Status,
        pub transcript: String,
        pub message: Option<String>,
        pub transcript_bytes: usize,
        pub transcript_chars: usize,
    }

    impl Default for State {
        fn default() -> Self {
            Self {
                status: Status::Idle,
                transcript: String::new(),
                message: None,
                transcript_bytes: 0,
                transcript_chars: 0,
            }
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

    #[derive(Debug, Default)]
    pub struct State {
        pub mode: Mode,
        pub input: String,
        pub custom_instruction: String,
        pub result: String,
        pub error: Option<String>,
        pub action: Option<Action>,
        pub can_replace: bool,
        pub captured_app: Option<String>,
        pub pending: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
        pub input_bytes: usize,
        pub input_chars: usize,
        pub result_bytes: usize,
        pub result_chars: usize,
        pub copied_at: Option<std::time::Instant>,
    }
}

pub mod settings_surface {
    #[derive(Debug, Default)]
    pub struct State {
        pub api_key: String,
        pub show_key: bool,
        pub status: Option<String>,
        pub testing: bool,
        pub test_result: Option<Result<String, String>>,
        pub test_pending: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    }
}

pub mod onboarding {
    #[derive(Clone, Debug, Default)]
    pub struct State {
        pub step: usize,
    }
    pub const STEPS: [&str; 3] = ["Welcome", "Shortcuts", "AI key"];
}
