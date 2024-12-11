#[rustversion::attr(nightly, ignore)]
#[test]
fn test_tool() {
    let t = trybuild::TestCases::new();
    t.pass("tests/tool/tool_single_argument_pass.rs");
    t.pass("tests/tool/tool_no_argument_pass.rs");
    t.pass("tests/tool/tool_multiple_arguments_pass.rs");
    t.compile_fail("tests/tool/tool_missing_arg_fail.rs");
    t.compile_fail("tests/tool/tool_missing_parameter_fail.rs");
}

#[rustversion::attr(nightly, ignore)]
#[test]
fn test_tool_derive() {
    let t = trybuild::TestCases::new();
    t.pass("tests/tool/tool_derive_pass.rs");
    t.compile_fail("tests/tool/tool_derive_missing_description.rs");
}
