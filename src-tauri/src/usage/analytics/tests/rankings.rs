use super::*;

#[test]
fn rankings_keep_unknown_groups_visible() {
    let store = seeded_store();
    let models = store
        .model_ranking(&AnalyticsFilter::default(), 10)
        .expect("models");
    assert_eq!(models[0].key.as_deref(), Some("gpt-6-astra"));
    assert_eq!(models[0].total_tokens, 2_300);
    assert_eq!(models[1].key.as_deref(), Some("gpt-5.6-luna"));
    let unknown = models
        .iter()
        .find(|entry| entry.key.is_none())
        .expect("unknown model stays visible");
    assert_eq!(unknown.sessions, 1);

    let projects = store
        .project_ranking(&AnalyticsFilter::default(), 10)
        .expect("projects");
    assert_eq!(projects[0].key.as_deref(), Some(PROJECT_BETA));
    assert_eq!(projects[0].total_tokens, 2_300);
    let alpha = projects
        .iter()
        .find(|entry| entry.key.as_deref() == Some(PROJECT_ALPHA))
        .expect("alpha");
    assert_eq!(alpha.sessions, 2, "跨客户端按项目聚合");
    assert_eq!(alpha.messages, 2);
}
