use glob::glob;

fn main() {
  let proto_dir = "proto/";

  // Collect all .proto files (including subdirectories)
  let proto_files: Vec<String> = glob(&format!("{}/**/*.proto", proto_dir))
    .unwrap()
    .filter_map(Result::ok)
    .map(|p| p.to_str().unwrap().to_string())
    .collect();

  // Compile all found .proto files
  prost_build::compile_protos(&proto_files, &[proto_dir]).unwrap();
}
