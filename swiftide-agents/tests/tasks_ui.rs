#[test]
fn task_type_invariants_are_enforced() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
