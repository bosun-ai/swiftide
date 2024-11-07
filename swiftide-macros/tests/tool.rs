#[test]
fn test_tool() {
    let t = trybuild::TestCases::new();
    t.pass("tests/tool/tool_basic_pass.rs");
}
