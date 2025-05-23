#[macro_export]
macro_rules! print_help {
    () => {
        $crate::cli_parser::Args::print_help()
    };
}


#[macro_export]
macro_rules! set_priority {
    ($priority:ident) => {
        let priority_lower: String = stringify!($priority).to_lowercase();
        let priority: windows::Win32::System::Threading::THREAD_PRIORITY = match priority_lower.as_str() {
            "idle" => windows::Win32::System::Threading::THREAD_PRIORITY_IDLE,
            "lowest" => windows::Win32::System::Threading::THREAD_PRIORITY_LOWEST,
            "min" => windows::Win32::System::Threading::THREAD_PRIORITY_MIN,
            "below_normal" => windows::Win32::System::Threading::THREAD_PRIORITY_BELOW_NORMAL,
            "normal" => windows::Win32::System::Threading::THREAD_PRIORITY_NORMAL,
            "above_normal" => windows::Win32::System::Threading::THREAD_PRIORITY_ABOVE_NORMAL,
            "highest" => windows::Win32::System::Threading::THREAD_PRIORITY_HIGHEST,
            "time_critical" => windows::Win32::System::Threading::THREAD_PRIORITY_TIME_CRITICAL,
            _ => panic!("Invalid priority level: {}", stringify!($priority))
        };
        unsafe {
            drop(windows::Win32::System::Threading::SetThreadPriority(
                windows::Win32::System::Threading::GetCurrentThread(),
                priority,
            ));
        }
    };
}
