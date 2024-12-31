//! Metadata is a key-value store for indexation nodes
//!
//! Typically metadata is used to extract or generate additional information about the node
//!
//! Internally it uses a `BTreeMap` to store the key-value pairs, to ensure the data is sorted.
use std::collections::{btree_map::IntoValues, BTreeMap};

use serde::Deserializer;

use crate::util::debug_long_utf8;

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    inner: BTreeMap<String, serde_json::Value>,
}

impl std::fmt::Debug for Metadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(
                self.inner
                    .iter()
                    .map(|(k, v): (&String, &serde_json::Value)| {
                        let fvalue = v.as_str().map_or_else(
                            || debug_long_utf8(v.to_string(), 100),
                            ToString::to_string,
                        );

                        (k, fvalue)
                    }),
            )
            .finish()
    }
}

impl Metadata {
    pub fn iter(&self) -> impl Iterator<Item = (&String, &serde_json::Value)> {
        self.inner.iter()
    }

    pub fn insert<K, V>(&mut self, key: K, value: V)
    where
        K: Into<String>,
        V: Into<serde_json::Value>,
    {
        self.inner.insert(key.into(), value.into());
    }

    pub fn get(&self, key: impl AsRef<str>) -> Option<&serde_json::Value> {
        self.inner.get(key.as_ref())
    }

    pub fn into_values(self) -> IntoValues<String, serde_json::Value> {
        self.inner.into_values()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl<K, V> Extend<(K, V)> for Metadata
where
    K: Into<String>,
    V: Into<serde_json::Value>,
{
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        self.inner
            .extend(iter.into_iter().map(|(k, v)| (k.into(), v.into())));
    }
}

impl<K, V> From<Vec<(K, V)>> for Metadata
where
    K: Into<String>,
    V: Into<serde_json::Value>,
{
    fn from(items: Vec<(K, V)>) -> Self {
        let inner = items
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        Metadata { inner }
    }
}

impl<K, V> From<(K, V)> for Metadata
where
    K: Into<String>,
    V: Into<serde_json::Value>,
{
    fn from(items: (K, V)) -> Self {
        let sliced: [(K, V); 1] = [items];
        let inner = sliced
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        Metadata { inner }
    }
}

impl<'a, K, V> From<&'a [(K, V)]> for Metadata
where
    K: Into<String> + Clone,
    V: Into<serde_json::Value> + Clone,
{
    fn from(items: &'a [(K, V)]) -> Self {
        let inner = items
            .iter()
            .cloned()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        Metadata { inner }
    }
}

impl<K: Ord, V, const N: usize> From<[(K, V); N]> for Metadata
where
    K: Ord + Into<String>,
    V: Into<serde_json::Value>,
{
    fn from(mut arr: [(K, V); N]) -> Self {
        if N == 0 {
            return Metadata {
                inner: BTreeMap::new(),
            };
        }
        arr.sort_by(|a, b| a.0.cmp(&b.0));
        let inner: BTreeMap<String, serde_json::Value> =
            arr.into_iter().map(|(k, v)| (k.into(), v.into())).collect();
        Metadata { inner }
    }
}

impl IntoIterator for Metadata {
    type Item = (String, serde_json::Value);
    type IntoIter = std::collections::btree_map::IntoIter<String, serde_json::Value>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'iter> IntoIterator for &'iter Metadata {
    type Item = (&'iter String, &'iter serde_json::Value);
    type IntoIter = std::collections::btree_map::Iter<'iter, String, serde_json::Value>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<'de> serde::Deserialize<'de> for Metadata {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        BTreeMap::deserialize(deserializer).map(|inner| Metadata { inner })
    }
}

impl serde::Serialize for Metadata {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.inner.serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_insert_and_get() {
        let mut metadata = Metadata::default();
        let key = "key";
        let value = "value";
        metadata.insert(key, "value");

        assert_eq!(metadata.get(key).unwrap().as_str(), Some(value));
    }

    #[test]
    fn test_iter() {
        let mut metadata = Metadata::default();
        metadata.insert("key1", json!("value1"));
        metadata.insert("key2", json!("value2"));

        let mut iter = metadata.iter();
        assert_eq!(iter.next(), Some((&"key1".to_string(), &json!("value1"))));
        assert_eq!(iter.next(), Some((&"key2".to_string(), &json!("value2"))));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_extend() {
        let mut metadata = Metadata::default();
        metadata.extend(vec![("key1", json!("value1")), ("key2", json!("value2"))]);

        assert_eq!(metadata.get("key1"), Some(&json!("value1")));
        assert_eq!(metadata.get("key2"), Some(&json!("value2")));
    }

    #[test]
    fn test_from_vec() {
        let metadata = Metadata::from(vec![("key1", json!("value1")), ("key2", json!("value2"))]);

        assert_eq!(metadata.get("key1"), Some(&json!("value1")));
        assert_eq!(metadata.get("key2"), Some(&json!("value2")));
    }

    #[test]
    fn test_into_values() {
        let mut metadata = Metadata::default();
        metadata.insert("key1", json!("value1"));
        metadata.insert("key2", json!("value2"));

        let values: Vec<_> = metadata.into_values().collect();
        assert_eq!(values, vec![json!("value1"), json!("value2")]);
    }
}
