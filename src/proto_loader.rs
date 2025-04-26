use anyhow::{Context, Result};
use prost_reflect::{DescriptorPool, MessageDescriptor};
use std::fs;
use std::path::Path;

pub struct ProtoLoader {
    pool: DescriptorPool,
}

impl ProtoLoader {
    pub fn new() -> Self {
        ProtoLoader {
            pool: DescriptorPool::new(),
        }
    }

    /// Load a .proto file from the given path
    pub fn load_proto_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        // let proto_source = fs::read_to_string(&path)
        //     .with_context(|| format!("Failed to read proto file: {:?}", path.as_ref()))?;

        // Use protoc to compile the proto file
        let protoc = protoc_bin_vendored::protoc_bin_path()?;
        let include_path = path.as_ref().parent().unwrap_or_else(|| Path::new("."));

        // Create a temporary directory for the output
        let temp_dir = tempfile::tempdir()?;
        let output_path = temp_dir.path().join("descriptor.bin");

        // Run protoc to generate the file descriptor set
        let status = std::process::Command::new(protoc)
            .arg("--include_imports")
            .arg(format!("--proto_path={}", include_path.display()))
            .arg(format!("--descriptor_set_out={}", output_path.display()))
            .arg(path.as_ref())
            .status()
            .with_context(|| "Failed to execute protoc")?;

        if !status.success() {
            anyhow::bail!("protoc failed with exit code: {}", status);
        }

        // Read the generated file descriptor set
        let descriptor_bytes = fs::read(&output_path)
            .with_context(|| format!("Failed to read descriptor file: {:?}", output_path))?;

        // Add the file descriptor set to the pool
        self.pool
            .decode_file_descriptor_set(&descriptor_bytes[..])?;

        Ok(())
    }

    /// Get a message descriptor by its fully qualified name
    pub fn get_message_descriptor(&self, message_name: &str) -> Result<MessageDescriptor> {
        self.pool
            .get_message_by_name(message_name)
            .with_context(|| format!("Message type not found: {}", message_name))
    }
}
