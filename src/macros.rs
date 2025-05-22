#[macro_export]
macro_rules! print_help {
    () => {
        $crate::cli_parser::Args::print_help()
    };
}
