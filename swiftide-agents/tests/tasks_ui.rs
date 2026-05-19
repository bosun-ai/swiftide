#[test]
fn task_type_invariants_are_enforced() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui-pass/*.rs");
    t.compile_fail("tests/ui/*.rs");
}
