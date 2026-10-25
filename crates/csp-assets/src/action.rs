//! Auto actions (オートアクション): CSP's own action sets
//! (`Action/*.laf`) and exported ones (`.las`). Both are [`crate::serial`]
//! documents:
//!
//! ```text
//! typeActionSet: actionSetName, applicationUuid, actionArray
//!   typeAction: actionName, actionUuid, actionOpenArray (bool: expanded
//!               in the palette), actionItemArray
//!     typeActionItem: actionItemKind (the command, e.g.
//!                     "layeractionrasterdetailnew" = 新規ラスターレイヤー),
//!                     actionItemEnable (the tick box),
//!                     actionItemShowSettings (show the dialog when run),
//!                     actionItemOpenParameter (bool), actionItemParameter
//!                     (an object; members depend on the command)
//! ```
//!
//! The command list and their parameters are in
//! `docs/assets/spec/action-kinds.tsv`. Edits go through the document, so
//! everything this module does not model is written back unchanged.

use serde::Serialize;
use serde_json::Value as Json;

use crate::error::{Error, Result};
use crate::serial::{Document, Object, Value};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActionItem {
    pub kind: String,
    pub enabled: bool,
    pub show_settings: bool,
    /// The command's parameters (serialisation JSON form).
    pub parameter: Json,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Action {
    pub name: String,
    pub uuid: String,
    pub items: Vec<ActionItem>,
}

/// An auto action set, with the document it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionSet {
    pub document: Document,
}

fn bool_of(object: &Object, key: &str) -> bool {
    object.i64(key).unwrap_or(0) != 0
}

impl ActionSet {
    pub fn parse(data: &[u8]) -> Result<ActionSet> {
        let document = Document::parse(data)?;
        if document.root.array("actionArray").is_none() {
            return Err(Error::format("auto action set: no actionArray"));
        }
        Ok(ActionSet { document })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.document.to_bytes()
    }

    pub fn name(&self) -> &str {
        self.document.root.str("actionSetName").unwrap_or("")
    }

    pub fn set_name(&mut self, name: &str) {
        self.document.root.set("actionSetName", Value::String(name.into()));
    }

    pub fn actions(&self) -> Vec<Action> {
        let Some(actions) = self.document.root.array("actionArray") else { return Vec::new() };
        actions
            .iter()
            .filter_map(Value::as_object)
            .map(|action| Action {
                name: action.str("actionName").unwrap_or("").into(),
                uuid: action.str("actionUuid").unwrap_or("").into(),
                items: action
                    .array("actionItemArray")
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(Value::as_object)
                    .map(|item| ActionItem {
                        kind: item.str("actionItemKind").unwrap_or("").into(),
                        enabled: bool_of(item, "actionItemEnable"),
                        show_settings: bool_of(item, "actionItemShowSettings"),
                        parameter: item
                            .object("actionItemParameter")
                            .map(|p| Document { root: p.clone(), trailing: Vec::new() }.to_json())
                            .unwrap_or(Json::Null),
                    })
                    .collect(),
            })
            .collect()
    }

    fn action_objects_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self.document.root.get_mut("actionArray")? {
            Value::Array { items, .. } => Some(items),
            _ => None,
        }
    }

    /// Tick or untick one item (action index, item index).
    pub fn set_enabled(&mut self, action: usize, item: usize, enabled: bool) -> Result<()> {
        let actions = self.action_objects_mut().ok_or_else(|| Error::format("no actions"))?;
        let Some(Value::Object(action)) = actions.get_mut(action) else {
            return Err(Error::format("no such action"));
        };
        let Some(Value::Array { items, .. }) = action.get_mut("actionItemArray") else {
            return Err(Error::format("action has no items"));
        };
        let Some(Value::Object(item)) = items.get_mut(item) else {
            return Err(Error::format("no such item"));
        };
        item.set("actionItemEnable", Value::bool(enabled));
        Ok(())
    }

    /// Keep only the actions for which `keep(index, name)` is true.
    pub fn retain_actions(&mut self, mut keep: impl FnMut(usize, &str) -> bool) {
        if let Some(actions) = self.action_objects_mut() {
            let mut index = 0;
            actions.retain(|a| {
                let name = a.as_object().and_then(|o| o.str("actionName")).unwrap_or("").to_owned();
                let result = keep(index, &name);
                index += 1;
                result
            });
        }
    }

    /// Append the actions of another set (CSP needs distinct uuids; actions
    /// whose uuid is already present are skipped).
    pub fn append_from(&mut self, other: &ActionSet) -> usize {
        let present: Vec<String> = self.actions().into_iter().map(|a| a.uuid).collect();
        let incoming: Vec<Value> = other
            .document
            .root
            .array("actionArray")
            .unwrap_or(&[])
            .iter()
            .filter(|a| {
                a.as_object().and_then(|o| o.str("actionUuid")).is_some_and(|u| !present.iter().any(|p| p == u))
            })
            .cloned()
            .collect();
        let count = incoming.len();
        if let Some(actions) = self.action_objects_mut() {
            actions.extend(incoming);
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_edits() {
        let mut set = ActionSet::parse(&crate::testing::action_set().to_bytes()).unwrap();
        assert_eq!(set.name(), "Test");
        let actions = set.actions();
        assert_eq!(actions[0].items[0].kind, "layeractionrasterdetailnew");
        assert!(actions[0].items[0].enabled);
        assert_eq!(actions[0].items[0].parameter["layername"], "下描き");
        set.set_enabled(0, 0, false).unwrap();
        set.set_name("Renamed");
        let again = ActionSet::parse(&set.to_bytes()).unwrap();
        assert!(!again.actions()[0].items[0].enabled);
        assert_eq!(again.name(), "Renamed");
        let mut other = crate::testing::action_set();
        assert_eq!(other.append_from(&again), 0);
        other.retain_actions(|_, _| false);
        assert!(other.actions().is_empty());
        assert_eq!(other.append_from(&again), 1);
    }
}
