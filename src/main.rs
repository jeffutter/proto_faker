mod distribution;
mod option_parser;
mod proto_faker;
mod proto_loader;

use anyhow::{Context, Result};
use bytes::Bytes;
use clap::{Parser, Subcommand};
use prost_reflect::prost::Message;
use prost_reflect::{DynamicMessage, MessageDescriptor, ReflectMessage, Value};
use proto_faker::ProtoFaker;
use proto_loader::ProtoLoader;
use rdkafka::config::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use schema_registry_converter::async_impl::proto_raw::ProtoRawEncoder;
use schema_registry_converter::async_impl::schema_registry::{self, SrSettings};
use schema_registry_converter::schema_registry_common::{SubjectNameStrategy, SuppliedSchema};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(author, version, about = "Generate random protobuf messages")]
struct Args {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(clap::Args, Debug, PartialEq)]
struct Common {
    /// Path to the .proto file
    #[arg(short = 'f', long)]
    proto_file: PathBuf,

    /// Message type to generate (fully qualified name)
    #[arg(short, long)]
    message_type: String,

    /// Number of messages to generate
    #[arg(short, long, default_value_t = 1)]
    count: usize,

    #[arg(short, long, value_parser = option_parser::parse_pool_config)]
    pools: Option<Vec<PoolConfig>>,
}

#[derive(Clone, Debug, PartialEq)]
struct PoolConfig {
    name: String,
    items: usize,
    value: option_parser::ValueType,
}

#[derive(Subcommand, Debug, PartialEq)]
enum Commands {
    Print {
        #[command(flatten)]
        common: Common,
    },
    Publish {
        #[command(flatten)]
        common: Common,

        /// Kafka broker address (required if publish is set)
        #[arg(short, long)]
        broker: String,

        /// Kafka topic to publish to (required if publish is set)
        #[arg(short, long)]
        topic: String,

        /// Schema registry URL (required if publish is set)
        #[arg(short, long)]
        schema_registry: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let common = match &args.cmd {
        Commands::Print { common } => common,
        Commands::Publish { common, .. } => common,
    };

    println!("Loading proto file: {}", common.proto_file.display());
    println!("Using message type: {}", common.message_type);
    println!("Generating {} message(s)", common.count);

    // Load the proto file
    let mut loader = ProtoLoader::new();
    loader.load_proto_file(&common.proto_file)?;

    // Get the message descriptor
    let message_descriptor = loader.get_message_descriptor(&common.message_type)?;
    println!("Found message type: {}", message_descriptor.full_name());

    let faker = ProtoFaker::new(common.pools.clone().unwrap_or(vec![]));

    let messages = std::iter::repeat_n((), common.count).map(|_| {
        let message = faker
            .generate_dynamic(&loader, &message_descriptor)
            .unwrap();

        message.encode_to_vec()
    });

    match args.cmd {
        Commands::Publish {
            broker,
            topic,
            schema_registry,
            common,
        } => {
            let producer = ClientConfig::new()
                .set("bootstrap.servers", broker)
                .set("message.timeout.ms", "5000")
                .create::<FutureProducer>()
                .context("Failed to create Kafka producer")?;

            let sr_settings = SrSettings::new(schema_registry);

            let schema = SuppliedSchema {
                name: Some(message_descriptor.full_name().to_string()),
                schema_type:
                    schema_registry_converter::schema_registry_common::SchemaType::Protobuf,
                schema: fs::read_to_string(common.proto_file).context("Can't read proto file")?,
                references: vec![],
            };

            schema_registry::post_schema(
                &sr_settings,
                message_descriptor.full_name().to_string(),
                schema,
            )
            .await
            .context("Failed to publish schema")?;

            let encoder = ProtoRawEncoder::new(sr_settings);

            let fs = messages.into_iter().enumerate().map(|(i, encoded)| {
                let encoded = encoded.clone();

                publish_to_kafka(
                    &producer,
                    &encoder,
                    &topic,
                    message_descriptor.full_name(),
                    encoded,
                    i,
                )
            });

            futures::future::try_join_all(fs).await?;
        }
        Commands::Print { common } => {
            for (i, encoded) in messages.enumerate() {
                if common.count > 1 {
                    println!("\n--- Message {} ---", i + 1);
                }
                let decoded = DynamicMessage::decode(
                    message_descriptor.clone(),
                    Bytes::from(encoded.clone()),
                )?;

                println!("Decoded message:");
                for field in message_descriptor.fields() {
                    let value = decoded.get_field(&field);
                    print_field_value(&message_descriptor, field.name(), &value, 2);
                }
            }
        }
    }

    Ok(())
}

async fn publish_to_kafka(
    producer: &FutureProducer,
    encoder: &ProtoRawEncoder<'_>,
    topic: &str,
    schema_name: &str,
    payload: Vec<u8>,
    message_index: usize,
) -> Result<()> {
    // Create a subject name strategy for the schema registry
    let subject_name_strategy = SubjectNameStrategy::RecordNameStrategy(schema_name.to_string());

    // Encode the message with the schema registry
    let encoded_payload = encoder
        .encode(&payload, schema_name, subject_name_strategy)
        .await
        .context("Failed to encode message with schema registry")?;

    let key = format!("key-{}", message_index);

    // Create a record to send
    let record = FutureRecord::to(topic).payload(&encoded_payload).key(&key);

    // Send the record
    let delivery_status = producer
        .send(record, Duration::from_secs(5))
        .await
        .map_err(|(kafka_error, _message)| kafka_error)
        .context("Failed to send message to Kafka")?;

    println!(
        "Message published to topic {} at partition {} with offset {}",
        topic, delivery_status.0, delivery_status.1
    );

    Ok(())
}

fn print_field_value(
    message_descriptor: &MessageDescriptor,
    name: &str,
    value: &Value,
    indent: usize,
) {
    let indent_str = " ".repeat(indent);

    match value {
        Value::Message(msg) => {
            println!("{}{}:", indent_str, name);
            for field in msg.descriptor().fields() {
                let field_value = msg.get_field(&field);
                print_field_value(&msg.descriptor(), field.name(), &field_value, indent + 2);
            }
        }
        Value::List(values) => {
            println!("{}{}:", indent_str, name);
            for (i, val) in values.iter().enumerate() {
                print_field_value(message_descriptor, &format!("[{}]", i), val, indent + 2);
            }
        }
        Value::EnumNumber(value) => {
            if let Some(field) = message_descriptor.fields().find(|f| f.name() == name) {
                // Check if this field is an enum type
                if let prost_reflect::Kind::Enum(enum_descriptor) = field.kind() {
                    if let Some(enum_value) =
                        enum_descriptor.values().find(|v| v.number() == *value)
                    {
                        println!("{}{}: {} ({})", indent_str, name, enum_value.name(), value);
                        return;
                    }
                }
            }
            // Fallback if we can't find the enum descriptor or value
            println!("{}{}: ENUM_VALUE ({})", indent_str, name, value);
        }
        _ => println!("{}{}: {:?}", indent_str, name, value),
    }
}
