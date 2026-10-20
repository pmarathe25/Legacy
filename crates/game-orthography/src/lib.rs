//! Pure reversible encoding of finite statements into an invented symbol inventory.
pub fn encode(alphabet: &[u64], bytes: &[u8]) -> Option<Vec<u64>> {
    bytes
        .iter()
        .map(|b| alphabet.get(usize::from(*b)).copied())
        .collect()
}
pub fn decode(alphabet: &[u64], glyphs: &[u64]) -> Option<Vec<u8>> {
    glyphs
        .iter()
        .map(|glyph| {
            alphabet
                .iter()
                .position(|g| g == glyph)
                .and_then(|i| u8::try_from(i).ok())
        })
        .collect()
}
/// The encoded assertion retains exact provenance and all values, including contradictions.
pub fn write<T: serde::Serialize>(alphabet: &[u64], assertion: &T) -> Option<Vec<u64>> {
    encode(alphabet, &serde_json::to_vec(assertion).ok()?)
}
pub fn read<T: serde::de::DeserializeOwned>(alphabet: &[u64], glyphs: &[u64]) -> Option<T> {
    serde_json::from_slice(&decode(alphabet, glyphs)?).ok()
}
#[cfg(test)]
mod tests {
    #[test]
    fn distinct_assertions_round_trip_and_unfamiliar_symbols_fail() {
        let alphabet: Vec<_> = (1000..1256).collect();
        for bytes in [b"warm".as_slice(), b"cold", &[0, 255]] {
            let glyphs = super::encode(&alphabet, bytes).unwrap();
            assert_eq!(super::decode(&alphabet, &glyphs).unwrap(), bytes);
        }
        assert!(super::decode(&alphabet, &[9]).is_none());
        assert!(super::encode(&[], b"x").is_none());
    }
}
