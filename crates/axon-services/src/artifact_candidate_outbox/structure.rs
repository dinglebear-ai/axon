//! Conservative decoded-heap admission without allocating a JSON value tree.
use serde::de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
const LIMIT: u64 = 64 * 1024 * 1024;
#[derive(Debug)]
pub(super) enum Rejected {
    Complexity,
    Corrupt,
}
struct Charge {
    bytes: u64,
    exceeded: bool,
}
struct Probe<'a>(&'a mut Charge);
impl Charge {
    fn add<E: serde::de::Error>(&mut self, bytes: u64) -> Result<(), E> {
        self.bytes = self.bytes.saturating_add(bytes);
        if self.bytes > LIMIT {
            self.exceeded = true;
            return Err(E::custom("outbox decoded structure admission exceeded"));
        }
        Ok(())
    }
}
impl<'de> DeserializeSeed<'de> for Probe<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Probe<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("bounded outbox JSON")
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<(), E> {
        self.0.add(64)
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<(), E> {
        self.0.add(64)
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<(), E> {
        self.0.add(64)
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<(), E> {
        self.0.add(64)
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
        self.0.add(64)
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<(), E> {
        self.0.add(64 + (value.len() as u64).saturating_mul(2))
    }
    fn visit_borrowed_str<E: serde::de::Error>(self, value: &'de str) -> Result<(), E> {
        self.visit_str(value)
    }
    fn visit_string<E: serde::de::Error>(self, value: String) -> Result<(), E> {
        self.visit_str(&value)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<(), A::Error> {
        self.0.add(128)?;
        while sequence.next_element_seed(Probe(self.0))?.is_some() {}
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        // Covers a minimally occupied BTreeMap allocation; per-entry values
        // and keys cover further nodes, spare array capacity and string copies.
        self.0.add(1024)?;
        while map.next_key_seed(Probe(self.0))?.is_some() {
            map.next_value_seed(Probe(self.0))?;
        }
        Ok(())
    }
}
pub(super) fn charge(bytes: &[u8]) -> Result<u64, Rejected> {
    let mut state = Charge {
        bytes: 0,
        exceeded: false,
    };
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let parsed = Probe(&mut state)
        .deserialize(&mut deserializer)
        .and_then(|()| deserializer.end());
    match parsed {
        Ok(()) => Ok(state.bytes),
        Err(_) if state.exceeded => Err(Rejected::Complexity),
        Err(_) => Err(Rejected::Corrupt),
    }
}
pub(super) fn limit() -> u64 {
    LIMIT
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compressed_structure_is_admitted_by_heap_cost_not_only_wire_bytes() {
        let ordinary = serde_json::to_vec(&vec![0_u8; 100_000]).unwrap();
        assert!(charge(&ordinary).unwrap() > ordinary.len() as u64);
        let objects = format!(
            "[{}]",
            std::iter::repeat_n("{}", 70_000)
                .collect::<Vec<_>>()
                .join(",")
        );
        assert!(matches!(
            charge(objects.as_bytes()),
            Err(Rejected::Complexity)
        ));
        assert!(matches!(charge(b"invalid"), Err(Rejected::Corrupt)));
    }
}
