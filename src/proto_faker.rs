use anyhow::Result;
use fake::Fake;
use fake::faker::internet::en::SafeEmail;
use fake::faker::lorem::en::Sentence;
use fake::faker::name::en::Name;
use fake::faker::phone_number::en::PhoneNumber as FakePhoneNumber;
use prost_reflect::{DynamicMessage, FieldDescriptor, Kind, MessageDescriptor, Value};
use rand::Rng;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct ProtoFaker;

impl ProtoFaker {
    pub fn new() -> Self {
        ProtoFaker
    }

    /// Generate a random protobuf message based on its descriptor
    pub fn generate_dynamic(
        &self,
        message_descriptor: &MessageDescriptor,
    ) -> Result<DynamicMessage> {
        let mut message = DynamicMessage::new(message_descriptor.clone());

        for field in message_descriptor.fields() {
            if rand::random::<f32>() < 0.95 {
                // 95% chance to populate each field
                let value = self.generate_field_value(&field)?;
                message.set_field(&field, value);
            }
        }

        Ok(message)
    }

    /// Generate a random value for a field based on its type
    fn generate_field_value(&self, field: &FieldDescriptor) -> Result<Value> {
        let mut rng = rand::thread_rng();

        match field.kind() {
            Kind::Double => Ok(Value::F64(rng.r#gen_range(-1000.0..1000.0))),
            Kind::Float => Ok(Value::F32(rng.r#gen_range(-1000.0..1000.0))),
            Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => {
                Ok(Value::I32(rng.r#gen_range(-10000..10000)))
            }
            Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => {
                Ok(Value::I64(rng.r#gen_range(-10000..10000)))
            }
            Kind::Uint32 | Kind::Fixed32 => Ok(Value::U32(rng.r#gen_range(0..20000))),
            Kind::Uint64 | Kind::Fixed64 => Ok(Value::U64(rng.r#gen_range(0..20000))),
            Kind::Bool => Ok(Value::Bool(rng.r#gen_bool(0.5))),
            Kind::String => {
                // Choose appropriate fake data based on field name
                let field_name = field.name().to_lowercase();
                let value = if field_name.contains("name") {
                    Name().fake()
                } else if field_name.contains("email") {
                    SafeEmail().fake()
                } else if field_name.contains("phone") || field_name.contains("number") {
                    FakePhoneNumber().fake()
                } else {
                    // Default to a random sentence for other string fields
                    Sentence(1..3).fake()
                };
                Ok(Value::String(value))
            }
            Kind::Bytes => {
                // Generate random bytes
                let len = rng.r#gen_range(4..20);
                let bytes: Vec<u8> = (0..len).map(|_| rng.r#gen::<u8>()).collect();
                Ok(Value::Bytes(bytes.into()))
            }
            Kind::Message(message_type) => {
                if message_type.full_name() == "google.protobuf.Timestamp" {
                    // Special handling for Timestamp
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default();
                    let offset = rng.gen_range(-86400..86400); // +/- 1 day in seconds

                    let mut timestamp_msg = DynamicMessage::new(message_type);
                    timestamp_msg
                        .set_field_by_name("seconds", Value::I64((now.as_secs() as i64) + offset));
                    timestamp_msg
                        .set_field_by_name("nanos", Value::I32(rng.r#gen_range(0..999_999_999)));

                    Ok(Value::Message(timestamp_msg))
                } else {
                    // Recursively generate nested message
                    let nested_message = self.generate_dynamic(&message_type)?;
                    Ok(Value::Message(nested_message))
                }
            }
            Kind::Enum(enum_type) => {
                // Choose a random enum value
                let values = enum_type.values();
                let values: Vec<_> = values.collect();
                if !values.is_empty() {
                    let idx = rng.r#gen_range(0..values.len());
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
    use super::*;
    use crate::proto_loader::ProtoLoader;
    use prost::Message;

    #[test]
    fn test_generate_dynamic_message() -> Result<()> {
        let mut loader = ProtoLoader::new();
        loader.load_proto_file("proto/person.proto")?;

        let message_descriptor = loader.get_message_descriptor("person.Person")?;
        let faker = ProtoFaker::new();

        let message = faker.generate_dynamic(&message_descriptor)?;

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

        let faker = ProtoFaker::new();

        // Test Person message
        let person_descriptor = loader.get_message_descriptor("person.Person")?;
        let person = faker.generate_dynamic(&person_descriptor)?;
        assert!(person.get_field_by_name("name").is_some());

        // Test PhoneNumber message
        let phone_descriptor = loader.get_message_descriptor("person.Person.PhoneNumber")?;
        let phone = faker.generate_dynamic(&phone_descriptor)?;
        assert!(phone.get_field_by_name("number").is_some());

        Ok(())
    }
}
