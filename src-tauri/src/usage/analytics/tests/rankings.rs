use super::*;

#[test]
fn rankings_keep_unknown_groups_visible() {
    let store = seeded_store();
    let models = store
        .model_ranking(&AnalyticsFilter::default(), 10)
        .expect("models");
    // 没有请求明细的会话按客户端自报的累计值参与排行。
    assert_eq!(models[0].key.as_deref(), Some("claude-sonnet-5"));
    assert_eq!(models[0].total_tokens, 650);
    assert_eq!(models[1].key.as_deref(), Some("gpt-5.6-luna"));
    assert_eq!(models[1].total_tokens, 24);
    let unknown = models
        .iter()
        .find(|entry| entry.key.is_none())
        .expect("unknown model stays visible");
    assert_eq!(unknown.sessions, 1);

    let projects = store
        .project_ranking(&AnalyticsFilter::default(), 10)
        .expect("projects");
    assert_eq!(projects[0].key.as_deref(), Some(PROJECT_ALPHA));
    assert_eq!(projects[0].total_tokens, 650 + 24);
    let beta = projects
        .iter()
        .find(|entry| entry.key.as_deref() == Some(PROJECT_BETA))
        .expect("beta");
    assert_eq!(beta.total_tokens, 12);
    let alpha = projects
        .iter()
        .find(|entry| entry.key.as_deref() == Some(PROJECT_ALPHA))
        .expect("alpha");
    assert_eq!(alpha.sessions, 2, "跨客户端按项目聚合");
    assert_eq!(alpha.messages, 2);
}
