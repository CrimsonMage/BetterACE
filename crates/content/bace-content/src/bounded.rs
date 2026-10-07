use serde::{
    Deserialize, Deserializer,
    de::{Error, SeqAccess, Visitor},
};
use std::{fmt, marker::PhantomData};

/// Collection bounds are enforced while decoding, before untrusted length hints
/// can request large allocations. The aggregate validator also checks the total.
pub(crate) fn vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Bounded<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Bounded<T> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("at most 100000 content entries")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            if sequence.size_hint().is_some_and(|count| count > 100_000) {
                return Err(A::Error::custom(
                    "content collection exceeds 100000 entries",
                ));
            }
            let mut result = Vec::new();
            while let Some(value) = sequence.next_element()? {
                if result.len() >= 100_000 {
                    return Err(A::Error::custom(
                        "content collection exceeds 100000 entries",
                    ));
                }
                result.push(value);
            }
            Ok(result)
        }
    }
    deserializer.deserialize_seq(Bounded(PhantomData))
}
