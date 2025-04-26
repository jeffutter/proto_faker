mod proto_faker;
mod proto_loader;

use anyhow::Result;
use bytes::Bytes;
use prost_reflect::prost::Message;
use prost_reflect::{DynamicMessage, ReflectMessage, Value};
use proto_faker::ProtoFaker;
use proto_loader::ProtoLoader;
use std::env;

fn main() -> Result<()> {
    // Get proto file path from command line or use default
    let proto_path = env::args()
        .nth(1)
        .unwrap_or_else(|| "proto/person.proto".to_string());
    let message_type = env::args()
        .nth(2)
        .unwrap_or_else(|| "person.Person".to_string());

    println!("Loading proto file: {}", proto_path);
    println!("Using message type: {}", message_type);

    // Load the proto file
    let mut loader = ProtoLoader::new();
    loader.load_proto_file(&proto_path)?;

    // Get the message descriptor
    let message_descriptor = loader.get_message_descriptor(&message_type)?;
    println!("Found message type: {}", message_descriptor.full_name());

    // Create a faker and generate a random message
    let faker = ProtoFaker::new();
    let message = faker.generate_dynamic(&message_descriptor)?;

    // Encode the message to bytes
    let encoded = message.encode_to_vec();
    println!("Generated random message with {} bytes", encoded.len());

    // Decode and print the message
    let decoded = DynamicMessage::decode(message_descriptor.clone(), Bytes::from(encoded.clone()))?;
    println!("Decoded message:");

    // Print all fields in the message
    for field in message_descriptor.fields() {
        let value = decoded.get_field(&field);
        print_field_value(field.name(), &value, 2);
    }

    Ok(())
}

fn print_field_value(name: &str, value: &Value, indent: usize) {
    let indent_str = " ".repeat(indent);

    match value {
        Value::Message(msg) => {
            println!("{}{}:", indent_str, name);
            for field in msg.descriptor().fields() {
                let field_value = msg.get_field(&field);
                print_field_value(field.name(), &field_value, indent + 2);
            }
        }
        Value::List(values) => {
            println!("{}{}:", indent_str, name);
            for (i, val) in values.iter().enumerate() {
                print_field_value(&format!("[{}]", i), val, indent + 2);
            }
        }
        _ => println!("{}{}: {:?}", indent_str, name, value),
    }
}
