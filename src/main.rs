mod option_parser;
mod proto_faker;
mod proto_loader;

use anyhow::Result;
use bytes::Bytes;
use clap::Parser;
use prost_reflect::prost::Message;
use prost_reflect::{DynamicMessage, ReflectMessage, Value};
use proto_faker::ProtoFaker;
use proto_loader::ProtoLoader;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about = "Generate random protobuf messages")]
struct Args {
    /// Path to the .proto file
    #[arg(short, long, default_value = "proto/person.proto")]
    proto_file: PathBuf,

    /// Message type to generate (fully qualified name)
    #[arg(short, long, default_value = "person.Person")]
    message_type: String,

    /// Number of messages to generate
    #[arg(short, long, default_value_t = 1)]
    count: usize,
}

fn main() -> Result<()> {
    let args = Args::parse();

    println!("Loading proto file: {}", args.proto_file.display());
    println!("Using message type: {}", args.message_type);
    println!("Generating {} message(s)", args.count);

    // Load the proto file
    let mut loader = ProtoLoader::new();
    loader.load_proto_file(&args.proto_file)?;

    // Get the message descriptor
    let message_descriptor = loader.get_message_descriptor(&args.message_type)?;
    println!("Found message type: {}", message_descriptor.full_name());

    // Create a faker and generate a random message
    let faker = ProtoFaker::new();
    for i in 0..args.count {
        if args.count > 1 {
            println!("\n--- Message {} ---", i + 1);
        }
        let message = faker.generate_dynamic(&loader, &message_descriptor)?;

        // Encode the message to bytes
        let encoded = message.encode_to_vec();
        println!("Generated random message with {} bytes", encoded.len());

        // Decode and print the message
        let decoded =
            DynamicMessage::decode(message_descriptor.clone(), Bytes::from(encoded.clone()))?;
        println!("Decoded message:");

        // Print all fields in the message
        for field in message_descriptor.fields() {
            let value = decoded.get_field(&field);
            print_field_value(field.name(), &value, 2);
        }
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
