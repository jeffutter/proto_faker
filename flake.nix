{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
      crane,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        lib = nixpkgs.lib;
        craneLib = crane.mkLib pkgs;

        src = lib.cleanSourceWith { src = craneLib.path ./.; };

        envVars =
          { }
          // (lib.attrsets.optionalAttrs pkgs.stdenv.isLinux {
            RUSTFLAGS = "-Clinker=clang -Clink-arg=--ld-path=${pkgs.mold}/bin/mold";
          });

        commonArgs = (
          {
            inherit src;
            nativeBuildInputs = with pkgs; [
              rust-bin.stable.latest.default
              cargo
              clang
              rust-analyzer
              rustc
              rdkafka
              protobuf
            ];
            buildInputs = with pkgs; [ ] ++ lib.optionals stdenv.isDarwin [ libiconv ];
          }
          // envVars
        );
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        bin = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
          }
        );
        serverProperties = (
          pkgs.writeTextFile {
            name = "server.properties";
            text = ''
              advertised.listeners=PLAINTEXT://localhost:9092
              broker.id=0
              config.storage.replication.factor=1
              confluent.license.topic.replication.factor=1
              controller.listener.names=CONTROLLER
              controller.quorum.voters=0@localhost:9093
              default.replication.factor=1
              group.initial.rebalance.delay.ms=0
              listener.security.protocol.map=CONTROLLER:PLAINTEXT,PLAINTEXT:PLAINTEXT
              listeners=PLAINTEXT://localhost:9092,CONTROLLER://localhost:9093
              node.id=0
              num.partitions=3
              offsets.topic.replication.factor=1
              process.roles=broker,controller
              share.coordinator.state.topic.replication.factor=1
              status.storage.replication.factor=1
              transaction.state.log.min.isr=1
              transaction.state.log.replication.factor=1
            '';
          }
        );
        schemaRegistryProperties = (
          pkgs.writeTextFile {
            name = "schema-registry.properties";
            text = ''
              schema.registry.url=http://localhost:8081
              kafkastore.bootstrap.servers=localhost:9092
              kafkastore.security.protocol=PLAINTEXT
            '';
          }
        );
      in
      with pkgs;
      {
        packages = {
          default = bin;
        };

        devShells.default = mkShell (
          {
            packages = [
              rust-bin.stable.latest.default
              cargo
              cargo-watch
              rust-analyzer
              rustc
              rustfmt
              confluent-platform
              rdkafka
              protobuf
              (pkgs.writeShellScriptBin "start-kafka" ''
                #!/bin/bash

                path="$1"

                if [[ ! -d "''${path}" ]]; then
                  echo "First argument must be a path to a data directory"
                  exit 1
                fi

                ${confluent-platform}/bin/kafka-server-start ${serverProperties} --override log.dirs="''${path}/"
              '')
              (pkgs.writeShellScriptBin "start-schema-registry" ''
                #!/bin/bash
                ${confluent-platform}/bin/schema-registry-start ${schemaRegistryProperties}
              '')
              (pkgs.writeShellScriptBin "init-kafka-properties" ''
                #!/bin/bash

                path="$1"

                if [[ ! -d "''${path}" ]]; then
                  echo "First argument must be a path to a data directory"
                  exit 1
                fi

                cat <<EOF > "''${path}/meta.properties"
                cluster.id=$(${confluent-platform}/bin/kafka-storage random-uuid)
                node.id=0
                version=1
                EOF
              '')
            ];
          }
          // envVars
        );

        formatter = nixpkgs-fmt;
      }
    );
}
