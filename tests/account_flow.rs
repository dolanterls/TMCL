use tmcl::core::account::offline::create_account;
use tmcl::core::account::store::AccountStore;

#[test]
fn offline_account_round_trips_without_secret_fields() {
    let root = std::env::temp_dir().join(format!("tmcl-account-flow-{}", uuid::Uuid::new_v4()));
    let store = AccountStore::new(root.join("accounts.json"));
    let account = create_account("Builder", None).expect("offline account");
    store.add(account.clone()).expect("save account");
    let loaded = store.load().expect("load accounts");
    assert_eq!(loaded, vec![account]);
    let raw = std::fs::read_to_string(root.join("accounts.json")).expect("account JSON");
    assert!(!raw.contains("access_token"));
    assert!(!raw.contains("refresh_token"));
    std::fs::remove_dir_all(root).expect("clean account flow");
}
