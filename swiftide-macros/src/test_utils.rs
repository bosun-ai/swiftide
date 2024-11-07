pub fn pretty_macro_output(item: &proc_macro2::TokenStream) -> String {
    let file = syn::parse_file(&item.to_string())
        .unwrap_or_else(|_| panic!("Failed to parse token stream: {}", &item.to_string()));
    prettyplease::unparse(&file)
}

// Add a macro that pretty compares two token streams using the above called `assert_ts_eq!`
#[macro_export]
macro_rules! assert_ts_eq {
    ($left:expr, $right:expr) => {{
        let left_pretty = $crate::test_utils::pretty_macro_output(&$left);
        let right_pretty = $crate::test_utils::pretty_macro_output(&$right);
        pretty_assertions::assert_eq!(left_pretty, right_pretty);
    }};
}
