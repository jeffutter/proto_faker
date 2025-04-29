use anyhow::Result;
use fake::Fake;
use fake::faker::internet::en::SafeEmail;
use fake::faker::lorem::en::Sentence;
use fake::faker::name::en::Name;
use fake::faker::phone_number::en::PhoneNumber as FakePhoneNumber;
use prost_reflect::{DynamicMessage, FieldDescriptor, Kind, MessageDescriptor, Value};
use rand::seq::IndexedRandom;
use rand::{Rng, RngCore};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::option_parser::parse_options;
use crate::proto_loader::ProtoLoader;
use crate::{PoolConfig, distribution, option_parser};

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
            }
            // else if rand::random::<f32>() < 0.95 {
            //     // 95% chance to populate each non-repeated field
            //     let value = self.generate_field_value(&field, &options, loader)?;
            //     message.set_field(&field, value);
            // }
            let value = self.generate_field_value(&field, &options, loader)?;
            message.set_field(&field, value);
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
        let distr = options.get("distribution").and_then(|v| match v {
            option_parser::Value::Distribution(distribution) => Some(distribution),
            _ => None,
        });

        let mut rng: Box<dyn RngCore> = match distr {
            Some(option_parser::Distribution::Uniform) => Box::new(rand::rng()),
            Some(option_parser::Distribution::Pareto(scale, shape)) => {
                Box::new(distribution::ParetoRng::new(rand::rng(), *scale, *shape))
            }
            Some(option_parser::Distribution::Normal(scale, shape)) => {
                Box::new(distribution::NormalRng::new(rand::rng(), *scale, *shape))
            }
            Some(option_parser::Distribution::LogNormal(scale, shape)) => {
                Box::new(distribution::LogNormalRng::new(rand::rng(), *scale, *shape))
            }
            None => Box::new(rand::rng()),
        };

        match field.kind() {
            Kind::Double => Ok(Value::F64(rng.random_range(-1000.0..1000.0))),
            Kind::Float => Ok(Value::F32(rng.random_range(-1000.0..1000.0))),
            Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => {
                Ok(Value::I32(rng.random_range(-1000..1000)))
            }
            Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => {
                Ok(Value::I64(rng.random_range(-1000..1000)))
            }
            Kind::Uint32 | Kind::Fixed32 => Ok(Value::U32(rng.random_range(0..20000))),
            Kind::Uint64 | Kind::Fixed64 => Ok(Value::U64(rng.random_range(0..20000))),
            Kind::Bool => Ok(Value::Bool(rng.random_bool(0.5))),
            Kind::String => {
                if let Some(option_parser::Value::Str(s)) = options.get("pool") {
                    if let Some(pool) = self.pools.get(s) {
                        if let Some(v) = pool.choose(&mut rng) {
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
                        return Ok(Value::String(
                            Sentence(i as usize..i as usize).fake_with_rng(&mut rng),
                        ));
                    }
                    Some(&option_parser::Value::Range(s, e)) => {
                        return Ok(Value::String(
                            Sentence(s as usize..e as usize).fake_with_rng(&mut rng),
                        ));
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
                    return Ok(Value::String(fake::uuid::UUIDv4.fake_with_rng(&mut rng)));
                }

                match field_name {
                    s if s.contains("name") => Ok(Value::String(Name().fake_with_rng(&mut rng))),
                    s if s.contains("email") => {
                        Ok(Value::String(SafeEmail().fake_with_rng(&mut rng)))
                    }
                    s if s.contains("phone") || s.contains("number") => {
                        Ok(Value::String(FakePhoneNumber().fake_with_rng(&mut rng)))
                    }
                    _ => Ok(Value::String(Sentence(1..3).fake_with_rng(&mut rng))),
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
        let faker = ProtoFaker::new(vec![PoolConfig {
            name: String::from("user_id"),
            items: 20,
            value: option_parser::ValueType::Uuid,
        }]);

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

        let faker = ProtoFaker::new(vec![PoolConfig {
            name: String::from("user_id"),
            items: 20,
            value: option_parser::ValueType::Uuid,
        }]);

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
        let faker = ProtoFaker::new(vec![PoolConfig {
            name: String::from("user_id"),
            items: 20,
            value: option_parser::ValueType::Uuid,
        }]);

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
