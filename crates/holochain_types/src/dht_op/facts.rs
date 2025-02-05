//! Facts about DhtOps

use super::*;
use ::contrafact::*;
use holochain_keystore::MetaLairClient;

/// Fact: The DhtOp is internally consistent in all of its references:
/// - TODO: The DhtOp variant matches the Action variant
/// - The Signature matches the Action
/// - If the action references an Entry, the Entry will exist and be of the appropriate hash
/// - If the action does not reference an Entry, the entry will be None
pub fn valid_dht_op(
    keystore: MetaLairClient,
    author: AgentPubKey,
    must_be_public: bool,
) -> impl Fact<'static, DhtOp> {
    facts![
        brute(
            "Action type matches Entry existence, and is public if exists",
            move |op: &DhtOp| {
                let action = op.action();
                let h = action.entry_data();
                let e = op.entry();
                match (h, e) {
                    (
                        Some((_entry_hash, entry_type)),
                        RecordEntry::Present(_) | RecordEntry::NotStored,
                    ) => {
                        // Ensure that entries are public
                        !must_be_public || entry_type.visibility().is_public()
                    }
                    (None, RecordEntry::Present(_)) => false,
                    (None, _) => true,
                    _ => false,
                }
            }
        ),
        lambda_unit(
            "If there is entry data, the action must point to it",
            |g, op: DhtOp| {
                if let Some(entry) = op.entry().into_option() {
                    // NOTE: this could be a `lens` if the previous check were short-circuiting,
                    // but it is possible that this check will run even if the previous check fails,
                    // so use a prism instead.
                    prism(
                        "action's entry hash",
                        |op: &mut DhtOp| op.action_entry_data_mut().map(|(hash, _)| hash),
                        eq(EntryHash::with_data_sync(entry)),
                    )
                    .mutate(g, op)
                } else {
                    Ok(op)
                }
            }
        ),
        lens1(
            "The author is the one specified",
            DhtOp::author_mut,
            eq(author)
        ),
        lambda_unit("The Signature matches the Action", move |g, op: DhtOp| {
            use holochain_keystore::AgentPubKeyExt;
            let action = op.action();
            let agent = action.author();
            let actual = tokio_helper::block_forever_on(agent.sign(&keystore, &action))
                .expect("Can sign the action");
            lens1("signature", DhtOp::signature_mut, eq(actual)).mutate(g, op)
        })
    ]
}

#[cfg(test)]
mod tests {
    use arbitrary::Arbitrary;
    use holochain_keystore::test_keystore::spawn_test_keystore;

    use super::*;
    use holochain_zome_types::action::facts as action_facts;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_valid_dht_op() {
        // TODO: Must add constraint on dht op variant wrt action variant

        let mut gg = Generator::from(unstructured_noise());
        let g = &mut gg;
        let keystore = spawn_test_keystore().await.unwrap();
        let agent = AgentPubKey::new_random(&keystore).await.unwrap();

        let e = Entry::arbitrary(g).unwrap();

        let mut a0 = action_facts::is_not_entry_action().build(g);
        *a0.author_mut() = agent.clone();

        let mut a1 = action_facts::is_new_entry_action().build(g);
        *a1.entry_data_mut().unwrap().0 = EntryHash::with_data_sync(&e);
        let mut a1 = Action::from(a1);
        *a1.author_mut() = agent.clone();

        let sn = agent.sign(&keystore, &a0).await.unwrap();
        let se = agent.sign(&keystore, &a1).await.unwrap();

        let op0a = DhtOp::StoreRecord(sn.clone(), a0.clone(), RecordEntry::Present(e.clone()));
        let op0b = DhtOp::StoreRecord(sn.clone(), a0.clone(), RecordEntry::Hidden);
        let op0c = DhtOp::StoreRecord(sn.clone(), a0.clone(), RecordEntry::NA);
        let op0d = DhtOp::StoreRecord(sn.clone(), a0.clone(), RecordEntry::NotStored);

        let op1a = DhtOp::StoreRecord(se.clone(), a1.clone(), RecordEntry::Present(e.clone()));
        let op1b = DhtOp::StoreRecord(se.clone(), a1.clone(), RecordEntry::Hidden);
        let op1c = DhtOp::StoreRecord(se.clone(), a1.clone(), RecordEntry::NA);
        let op1d = DhtOp::StoreRecord(se.clone(), a1.clone(), RecordEntry::NotStored);

        let fact = valid_dht_op(keystore, agent, false);

        assert!(fact.clone().check(&op0a).is_err());
        fact.clone().check(&op0b).unwrap();
        fact.clone().check(&op0c).unwrap();
        fact.clone().check(&op0d).unwrap();

        fact.clone().check(&op1a).unwrap();
        assert!(fact.clone().check(&op1b).is_err());
        assert!(fact.clone().check(&op1c).is_err());
        fact.clone().check(&op1d).unwrap();
    }
}

impl DhtOp {
    /// Mutable access to the Author
    pub fn author_mut(&mut self) -> &mut AgentPubKey {
        match self {
            DhtOp::StoreRecord(_, h, _) => h.author_mut(),
            DhtOp::StoreEntry(_, h, _) => h.author_mut(),
            DhtOp::RegisterAgentActivity(_, h) => h.author_mut(),
            DhtOp::RegisterUpdatedContent(_, h, _) => &mut h.author,
            DhtOp::RegisterUpdatedRecord(_, h, _) => &mut h.author,
            DhtOp::RegisterDeletedBy(_, h) => &mut h.author,
            DhtOp::RegisterDeletedEntryAction(_, h) => &mut h.author,
            DhtOp::RegisterAddLink(_, h) => &mut h.author,
            DhtOp::RegisterRemoveLink(_, h) => &mut h.author,
        }
    }

    /// Access to the Timestamp
    pub fn timestamp(&self) -> Timestamp {
        match self {
            DhtOp::StoreRecord(_, h, _) => h.timestamp(),
            DhtOp::StoreEntry(_, h, _) => h.timestamp(),
            DhtOp::RegisterAgentActivity(_, h) => h.timestamp(),
            DhtOp::RegisterUpdatedContent(_, h, _) => h.timestamp,
            DhtOp::RegisterUpdatedRecord(_, h, _) => h.timestamp,
            DhtOp::RegisterDeletedBy(_, h) => h.timestamp,
            DhtOp::RegisterDeletedEntryAction(_, h) => h.timestamp,
            DhtOp::RegisterAddLink(_, h) => h.timestamp,
            DhtOp::RegisterRemoveLink(_, h) => h.timestamp,
        }
    }

    /// Mutable access to the Timestamp
    pub fn timestamp_mut(&mut self) -> &mut Timestamp {
        match self {
            DhtOp::StoreRecord(_, h, _) => h.timestamp_mut(),
            DhtOp::StoreEntry(_, h, _) => h.timestamp_mut(),
            DhtOp::RegisterAgentActivity(_, h) => h.timestamp_mut(),
            DhtOp::RegisterUpdatedContent(_, h, _) => &mut h.timestamp,
            DhtOp::RegisterUpdatedRecord(_, h, _) => &mut h.timestamp,
            DhtOp::RegisterDeletedBy(_, h) => &mut h.timestamp,
            DhtOp::RegisterDeletedEntryAction(_, h) => &mut h.timestamp,
            DhtOp::RegisterAddLink(_, h) => &mut h.timestamp,
            DhtOp::RegisterRemoveLink(_, h) => &mut h.timestamp,
        }
    }

    /// Mutable access to the Signature
    pub fn signature_mut(&mut self) -> &mut Signature {
        match self {
            DhtOp::StoreRecord(s, _, _) => s,
            DhtOp::StoreEntry(s, _, _) => s,
            DhtOp::RegisterAgentActivity(s, _) => s,
            DhtOp::RegisterUpdatedContent(s, _, _) => s,
            DhtOp::RegisterUpdatedRecord(s, _, _) => s,
            DhtOp::RegisterDeletedBy(s, _) => s,
            DhtOp::RegisterDeletedEntryAction(s, _) => s,
            DhtOp::RegisterAddLink(s, _) => s,
            DhtOp::RegisterRemoveLink(s, _) => s,
        }
    }

    /// Mutable access to the seq of the Action, if applicable
    pub fn action_seq_mut(&mut self) -> Option<&mut u32> {
        match self {
            DhtOp::StoreRecord(_, ref mut h, _) => h.action_seq_mut(),
            DhtOp::StoreEntry(_, ref mut h, _) => Some(h.action_seq_mut()),
            DhtOp::RegisterAgentActivity(_, ref mut h) => h.action_seq_mut(),
            DhtOp::RegisterUpdatedContent(_, ref mut h, _) => Some(&mut h.action_seq),
            DhtOp::RegisterUpdatedRecord(_, ref mut h, _) => Some(&mut h.action_seq),
            DhtOp::RegisterDeletedBy(_, ref mut h) => Some(&mut h.action_seq),
            DhtOp::RegisterDeletedEntryAction(_, ref mut h) => Some(&mut h.action_seq),
            DhtOp::RegisterAddLink(_, ref mut h) => Some(&mut h.action_seq),
            DhtOp::RegisterRemoveLink(_, ref mut h) => Some(&mut h.action_seq),
        }
    }

    /// Mutable access to the entry data of the Action, if applicable
    pub fn action_entry_data_mut(&mut self) -> Option<(&mut EntryHash, &mut EntryType)> {
        match self {
            DhtOp::StoreRecord(_, ref mut h, _) => h.entry_data_mut(),
            DhtOp::StoreEntry(_, ref mut h, _) => Some(h.entry_data_mut()),
            DhtOp::RegisterAgentActivity(_, ref mut h) => h.entry_data_mut(),
            DhtOp::RegisterUpdatedContent(_, ref mut h, _) => {
                Some((&mut h.entry_hash, &mut h.entry_type))
            }
            DhtOp::RegisterUpdatedRecord(_, ref mut h, _) => {
                Some((&mut h.entry_hash, &mut h.entry_type))
            }
            _ => None,
        }
    }
}
