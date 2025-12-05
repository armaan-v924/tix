use tix::commands::setup::sanitize_description;

#[test]
fn sanitize_basic_lowercases_and_separates() {
    assert_eq!(sanitize_description("Short Summary"), "short-summary");
    assert_eq!(sanitize_description("Feat: Payment/Auth"), "feat-payment-auth");
}

#[test]
fn sanitize_trims_repeated_and_edge_hyphens() {
    assert_eq!(sanitize_description("  weird---spacing  "), "weird-spacing");
    assert_eq!(sanitize_description("...Multi---Symbol--Case..."), "multi-symbol-case");
}

#[test]
fn sanitize_handles_empty_and_non_alnum() {
    assert_eq!(sanitize_description(""), "");
    assert_eq!(sanitize_description("###"), "");
}
