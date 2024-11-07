#[test]
fn test_tool() {
    let t = trybuild::TestCases::new();
    t.pass("tests/tool/tool_single_argument_pass.rs");
    t.pass("tests/tool/tool_no_argument_pass.rs");
}
