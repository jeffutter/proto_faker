use anyhow::Result;
use fake::Fake;
use fake::faker::internet::en::SafeEmail;
use fake::faker::lorem::en::Sentence;
use fake::faker::name::en::Name;
use fake::faker::phone_number::en::PhoneNumber as FakePhoneNumber;
use prost_reflect::{DynamicMessage, FieldDescriptor, Kind, MessageDescriptor, Value};
use rand::Rng;
use rand::rngs::ThreadRng;
use rand::seq::IndexedRandom;
use rand_distr::Distribution as _;
use std::collections::HashMap;
use std::ops::Range;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::option_parser::parse_options;
use crate::proto_loader::ProtoLoader;
use crate::{PoolConfig, option_parser};

pub struct ProtoFaker {
    pools: HashMap<String, Vec<Value>>,
}

impl ProtoFaker {
    pub fn new(pool_configs: Vec<PoolConfig>) -> Self {
        let mut pools = HashMap::new();

        for PoolConfig { name, items, value } in pool_configs {
            match value {
                option_parser::ValueType::I32 => pools.insert(
                    name,
                    fake::vec![i32; items].into_iter().map(Value::I32).collect(),
                ),
                option_parser::ValueType::I64 => pools.insert(
                    name,
                    fake::vec![i64; items].into_iter().map(Value::I64).collect(),
                ),
                option_parser::ValueType::U32 => pools.insert(
                    name,
                    fake::vec![u32; items].into_iter().map(Value::U32).collect(),
                ),
                option_parser::ValueType::U64 => pools.insert(
                    name,
                    fake::vec![u64; items].into_iter().map(Value::U64).collect(),
                ),
                option_parser::ValueType::F32 => pools.insert(
                    name,
                    fake::vec![f32; items].into_iter().map(Value::F32).collect(),
                ),
                option_parser::ValueType::F64 => pools.insert(
                    name,
                    fake::vec![f64; items].into_iter().map(Value::F64).collect(),
                ),
                option_parser::ValueType::String => pools.insert(
                    name,
                    fake::vec![String; items]
                        .into_iter()
                        .map(Value::String)
                        .collect(),
                ),
                option_parser::ValueType::Bytes => pools.insert(
                    name,
                    fake::vec![Vec<u8>; items]
                        .into_iter()
                        .map(|b| Value::Bytes(b.into()))
                        .collect(),
                ),
                option_parser::ValueType::Uuid => pools.insert(
                    name,
                    fake::vec![Uuid; items]
                        .into_iter()
                        .map(|u| Value::String(u.into()))
                        .collect(),
                ),
            };
        }

        ProtoFaker { pools }
    }

    /// Generate a random protobuf message based on its descriptor
    pub fn generate_dynamic(
        &self,
        loader: &ProtoLoader,
        message_descriptor: &MessageDescriptor,
    ) -> Result<DynamicMessage> {
        let mut message = DynamicMessage::new(message_descriptor.clone());
        let mut rng = rand::rng();

        for field in message_descriptor.fields() {
            let comment = loader.get_comment(
                message_descriptor.parent_file().name(),
                message_descriptor.name(),
                field.name(),
            )?;

            let options = comment.map(|p| parse_options(&p)).unwrap_or(HashMap::new());

            // Check if field is repeated by examining its cardinality
            let is_repeated = field.cardinality() == prost_reflect::Cardinality::Repeated;

            if is_repeated {
                let count = match options.get("count") {
                    Some(option_parser::Value::Int(i)) => *i..*i,
                    Some(option_parser::Value::Range(s, e)) => *s..*e,
                    None => 1..1,
                    _ => unimplemented!(),
                };
                let count = rng.random_range(count.start..count.end + 1);

                if count > 0 {
                    let mut values = Vec::new();
                    for _ in 0..count {
                        values.push(self.generate_field_value(&field, &options, loader)?);
                    }
                    message.set_field(&field, Value::List(values));
                }
            } else if rand::random::<f32>() < 0.95 {
                // 95% chance to populate each non-repeated field
                let value = self.generate_field_value(&field, &options, loader)?;
                message.set_field(&field, value);
            }
        }

        Ok(message)
    }

    /// Generate a random value for a field based on its type and attributes
    fn generate_field_value(
        &self,
        field: &FieldDescriptor,
        options: &HashMap<String, option_parser::Value>,
        loader: &ProtoLoader,
    ) -> Result<Value> {
        let mut rng = rand::rng();

        let distr = options.get("distribution").and_then(|v| match v {
            option_parser::Value::Distribution(distribution) => Some(distribution),
            _ => None,
        });

        match field.kind() {
            Kind::Double => Ok(Value::F64(range_rand_with_distribution(
                &mut rng,
                distr,
                -1000.0..1000.0,
            ))),
            Kind::Float => Ok(Value::F32(range_rand_with_distribution(
                &mut rng,
                distr,
                -1000.0..1000.0,
            ))),
            Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => Ok(Value::I32(
                range_rand_with_distribution(&mut rng, distr, -10000..10000),
            )),
            Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => Ok(Value::I64(
                range_rand_with_distribution(&mut rng, distr, -10000..10000),
            )),
            Kind::Uint32 | Kind::Fixed32 => Ok(Value::U32(range_rand_with_distribution(
                &mut rng,
                distr,
                0..20000,
            ))),
            Kind::Uint64 | Kind::Fixed64 => Ok(Value::U64(range_rand_with_distribution(
                &mut rng,
                distr,
                0..20000,
            ))),
            Kind::Bool => Ok(Value::Bool(rng.random_bool(0.5))),
            Kind::String => {
                if let Some(option_parser::Value::Str(s)) = options.get("pool") {
                    if let Some(pool) = self.pools.get(s) {
                        if let Some(v) = choose_rand_with_distribution(&mut rng, distr, pool) {
                            if let Value::String(s) = v {
                                return Ok(Value::String(s.clone()));
                            }
                            panic!(
                                "Specified Pool '{}' has wrong type on field {}",
                                s,
                                field.name()
                            )
                        }
                        panic!("Specified Pool '{}' is empty on field {}", s, field.name())
                    }
                    panic!("Specified Pool '{}' not found on field {}", s, field.name())
                }

                match options.get("words") {
                    Some(&option_parser::Value::Int(i)) => {
                        return Ok(Value::String(Sentence(i as usize..i as usize).fake()));
                    }
                    Some(&option_parser::Value::Range(s, e)) => {
                        return Ok(Value::String(Sentence(s as usize..e as usize).fake()));
                    }
                    Some(option_parser::Value::ListStr(l)) => {
                        return Ok(Value::String(l.choose(&mut rng).unwrap().clone()));
                    }
                    Some(_) => unimplemented!(),
                    None => (),
                }

                let field_name = field.name().to_lowercase();

                if Some(option_parser::Value::Str("uuid".to_string()))
                    == options.get("string").cloned()
                    || field_name == "uuid"
                    || field_name == "id"
                {
                    return Ok(Value::String(fake::uuid::UUIDv4.fake()));
                }

                match field_name {
                    s if s.contains("name") => Ok(Value::String(Name().fake())),
                    s if s.contains("email") => Ok(Value::String(SafeEmail().fake())),
                    s if s.contains("phone") || s.contains("number") => {
                        Ok(Value::String(FakePhoneNumber().fake()))
                    }
                    _ => Ok(Value::String(Sentence(1..3).fake())),
                }
            }
            Kind::Bytes => {
                // Generate random bytes
                let len = rng.random_range(4..20);
                let bytes: Vec<u8> = (0..len).map(|_| rng.random::<u8>()).collect();
                Ok(Value::Bytes(bytes.into()))
            }
            Kind::Message(message_type) => {
                if message_type.full_name() == "google.protobuf.Timestamp" {
                    // Special handling for Timestamp
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default();
                    let offset = rng.random_range(-86400..86400); // +/- 1 day in seconds

                    let mut timestamp_msg = DynamicMessage::new(message_type);
                    timestamp_msg
                        .set_field_by_name("seconds", Value::I64((now.as_secs() as i64) + offset));
                    timestamp_msg
                        .set_field_by_name("nanos", Value::I32(rng.random_range(0..999_999_999)));

                    Ok(Value::Message(timestamp_msg))
                } else {
                    // Recursively generate nested message
                    let nested_message = self.generate_dynamic(loader, &message_type)?;
                    Ok(Value::Message(nested_message))
                }
            }
            Kind::Enum(enum_type) => {
                // Choose a random enum value
                let values = enum_type.values();
                let values: Vec<_> = values.collect();
                if !values.is_empty() {
                    let idx = rng.random_range(0..values.len());
                    Ok(Value::EnumNumber(values[idx].number()))
                } else {
                    Ok(Value::EnumNumber(0)) // Default to 0 if no values
                }
            }
        }
    }
}

fn choose_rand_with_distribution<'a, T>(
    rng: &mut ThreadRng,
    distr: Option<&option_parser::Distribution>,
    list: &'a [T],
) -> Option<&'a T> {
    if list.is_empty() {
        return None;
    }

    match distr {
        Some(option_parser::Distribution::Uniform) => list.choose(rng),
        Some(option_parser::Distribution::Normal(mean, std_dev)) => {
            let len = list.len();
            let mean = (len - 1) as f64 / mean;
            let std_dev = (len as f64) / std_dev; // You can adjust this factor!

            let normal = rand_distr::Normal::new(mean, std_dev).unwrap();

            let mut sample = normal.sample(rng);

            // Normalize sample to [0, 1)
            sample = sample / (sample + 1.0);

            // Now map [0,1) to [0,len)
            let idx = (sample * list.len() as f64) as usize;
            let idx = idx.min(list.len() - 1); // Just in case sample == 1.0 exactly

            list.get(idx)
        }
        Some(option_parser::Distribution::LogNormal(mean, std_dev)) => {
            let lognormal = rand_distr::LogNormal::new(*mean, *std_dev).unwrap();

            let mut sample = lognormal.sample(rng);

            // Normalize sample to [0, 1)
            sample = sample / (sample + 1.0);

            // Now map [0,1) to [0,len)
            let idx = (sample * list.len() as f64) as usize;
            let idx = idx.min(list.len() - 1); // Just in case sample == 1.0 exactly

            list.get(idx)
        }
        Some(option_parser::Distribution::Pareto(scale, shape)) => {
            let pareto = rand_distr::Pareto::new(*scale, *shape).unwrap();

            let sample = pareto.sample(rng);

            // Convert it into an index: we can invert larger samples into smaller indices
            // (so lower indices are more probable if that's your intent).
            // Normalize sample by scale
            let normalized = sample / scale;

            // Map normalized value into an index
            let p = (1.0 / normalized).min(1.0); // Inverse + clamp to 1
            let idx = (p * (list.len() as f64)) as usize;

            // Clamp the index in case of rounding
            let idx = idx.min(list.len() - 1);

            list.get(idx)
        }
        None => list.choose(rng),
    }
}

fn range_rand_with_distribution<T>(
    rng: &mut ThreadRng,
    distr: Option<&option_parser::Distribution>,
    range: Range<T>,
) -> T
where
    T: Copy + PartialOrd + FromF64 + IntoF64 + rand_distr::uniform::SampleUniform,
{
    let start_f64 = to_f64(range.start);
    let end_f64 = to_f64(range.end);
    let len_f64 = end_f64 - start_f64;

    match distr {
        Some(option_parser::Distribution::Uniform) => rng.random_range(range),
        Some(option_parser::Distribution::Normal(mean, std_dev)) => {
            let mean = len_f64 / mean;
            let std_dev = len_f64 / std_dev;

            let normal = rand_distr::Normal::new(mean, std_dev).unwrap();
            let mut sample = normal.sample(rng);

            // Normalize to [0, 1)
            sample = sample / (sample + 1.0);

            // Scale to range
            let val = start_f64 + (sample * len_f64);
            T::from_f64(val)
        }
        Some(option_parser::Distribution::LogNormal(mean, std_dev)) => {
            let lognormal = rand_distr::LogNormal::new(*mean, *std_dev).unwrap();
            let mut sample = lognormal.sample(rng);

            sample = sample / (sample + 1.0);

            let val = start_f64 + (sample * len_f64);
            T::from_f64(val)
        }
        Some(option_parser::Distribution::Pareto(scale, shape)) => {
            let pareto = rand_distr::Pareto::new(*scale, *shape).unwrap();
            let sample = pareto.sample(rng);

            let normalized = sample / scale;
            let p = (1.0 / normalized).min(1.0);

            let val = start_f64 + (p * len_f64);
            T::from_f64(val)
        }
        None => rng.random_range(range),
    }
}

// Helper: Always convert T -> f64
fn to_f64<T: IntoF64>(val: T) -> f64 {
    val.into_f64()
}

/// A trait to unify "convert into f64" for different types
pub trait IntoF64 {
    fn into_f64(self) -> f64;
}

/// A trait to unify "convert from f64" for different types
pub trait FromF64 {
    fn from_f64(val: f64) -> Self;
}

// Implement for common types
impl IntoF64 for f32 {
    fn into_f64(self) -> f64 {
        self as f64
    }
}
impl IntoF64 for f64 {
    fn into_f64(self) -> f64 {
        self
    }
}
impl IntoF64 for u32 {
    fn into_f64(self) -> f64 {
        self as f64
    }
}
impl IntoF64 for u64 {
    fn into_f64(self) -> f64 {
        self as f64
    }
}
impl IntoF64 for usize {
    fn into_f64(self) -> f64 {
        self as f64
    }
}
impl IntoF64 for i32 {
    fn into_f64(self) -> f64 {
        self as f64
    }
}
impl IntoF64 for i64 {
    fn into_f64(self) -> f64 {
        self as f64
    }
}

// Implement FromF64 for common types
impl FromF64 for f32 {
    fn from_f64(val: f64) -> Self {
        val as f32
    }
}
impl FromF64 for f64 {
    fn from_f64(val: f64) -> Self {
        val
    }
}
impl FromF64 for u32 {
    fn from_f64(val: f64) -> Self {
        val.round().max(0.0) as u32
    }
}
impl FromF64 for u64 {
    fn from_f64(val: f64) -> Self {
        val.round().max(0.0) as u64
    }
}
impl FromF64 for usize {
    fn from_f64(val: f64) -> Self {
        val.round().max(0.0) as usize
    }
}
impl FromF64 for i32 {
    fn from_f64(val: f64) -> Self {
        val.round() as i32
    }
}
impl FromF64 for i64 {
    fn from_f64(val: f64) -> Self {
        val.round() as i64
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;
    use crate::proto_loader::ProtoLoader;
    use prost::Message;

    #[test]
    fn test_generate_dynamic_message() -> Result<()> {
        let mut loader = ProtoLoader::new();
        loader.load_proto_file("proto/person.proto")?;

        let message_descriptor = loader.get_message_descriptor("person.Person")?;
        let faker = ProtoFaker::new(vec![]);

        let message = faker.generate_dynamic(&loader, &message_descriptor)?;

        // Verify the message has data
        assert!(message.get_field_by_name("name").is_some());

        // Test serialization
        let encoded = message.encode_to_vec();
        assert!(!encoded.is_empty());

        // Test deserialization
        let decoded = DynamicMessage::decode(message_descriptor, encoded.as_slice())?;

        // Verify some fields match
        if let Some(name) = message.get_field_by_name("name") {
            if let Some(decoded_name) = decoded.get_field_by_name("name") {
                assert_eq!(name, decoded_name);
            }
        }

        Ok(())
    }

    #[test]
    fn test_generate_multiple_message_types() -> Result<()> {
        let mut loader = ProtoLoader::new();
        loader.load_proto_file("proto/person.proto")?;

        let faker = ProtoFaker::new(vec![]);

        // Test Person message
        let person_descriptor = loader.get_message_descriptor("person.Person")?;
        let person = faker.generate_dynamic(&loader, &person_descriptor)?;
        assert!(person.get_field_by_name("name").is_some());

        // Test PhoneNumber message
        let phone_descriptor = loader.get_message_descriptor("person.Person.PhoneNumber")?;
        let phone = faker.generate_dynamic(&loader, &phone_descriptor)?;
        assert!(phone.get_field_by_name("number").is_some());

        Ok(())
    }

    #[test]
    fn test_comment_attributes() -> Result<()> {
        let mut loader = ProtoLoader::new();
        loader.load_proto_file("proto/person.proto")?;

        let message_descriptor = loader.get_message_descriptor("person.Person")?;
        let faker = ProtoFaker::new(vec![]);

        let message = faker.generate_dynamic(&loader, &message_descriptor)?;

        // Check that name has 1-3 words
        if let Some(Cow::Borrowed(Value::String(name))) = message.get_field_by_name("name") {
            let word_count = name.split_whitespace().count();
            assert!(
                (1..=3).contains(&word_count),
                "Name '{}' should have 1-3 words, has {}",
                name,
                word_count
            );
        } else {
            panic!("No Name Field");
        }

        // Check that uuid is a valid UUID
        if let Some(Cow::Borrowed(Value::String(uuid_str))) = message.get_field_by_name("uuid") {
            assert!(
                Uuid::parse_str(uuid_str).is_ok(),
                "Invalid UUID: {}",
                uuid_str
            );
        } else {
            panic!("No UUID");
        }

        // Check that phones has 1-3 entries
        if let Some(Cow::Borrowed(Value::List(phones))) = message.get_field_by_name("phones") {
            assert!(
                !phones.is_empty() && phones.len() <= 3,
                "Phones should have 1-3 entries, has {}",
                phones.len()
            );
        } else {
            panic!("No phones");
        }

        Ok(())
    }
}
