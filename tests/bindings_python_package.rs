//! Tests for the installable Python package generator (#720).
//!
//! generate_python's existing single-file client-code output is unchanged
//! and covered by bindings_snapshots.rs; these tests cover the new
//! packaging layer (generate_python_package / write_package) that wraps it
//! into a directory a user can `pip install .`.

use starforge::utils::bindings::{
    complex_metadata, generate_python_package, write_package, PackageFile,
};

fn find<'a>(files: &'a [PackageFile], path: &str) -> &'a PackageFile {
    files
        .iter()
        .find(|f| f.relative_path == path)
        .unwrap_or_else(|| panic!("expected generated file {path}"))
}

#[test]
fn generates_pyproject_toml_with_valid_build_metadata() {
    let metadata = complex_metadata();
    let files = generate_python_package(&metadata, "My Contract");

    let pyproject = find(&files, "pyproject.toml");
    assert!(pyproject.contents.contains("[build-system]"));
    assert!(pyproject.contents.contains("[project]"));
    assert!(pyproject.contents.contains("name = \"my-contract\""));
    assert!(pyproject.contents.contains("requires-python = \">=3.10\""));
}

#[test]
fn generates_an_importable_init_module() {
    let metadata = complex_metadata();
    let files = generate_python_package(&metadata, "my_contract");

    let init = find(&files, "my_contract/__init__.py");
    assert!(init.contents.contains("from .client import ContractClient"));
    assert!(init.contents.contains("__all__"));
}

#[test]
fn client_module_contains_the_same_generated_client_code() {
    let metadata = complex_metadata();
    let files = generate_python_package(&metadata, "my_contract");

    let client = find(&files, "my_contract/client.py");
    assert!(client.contents.contains("class ContractClient"));
    assert!(client.contents.contains("@dataclass"));
}

#[test]
fn normalizes_package_name_for_distribution_and_module() {
    let metadata = complex_metadata();
    let files = generate_python_package(&metadata, "My Cool Contract!");

    // PEP 503 distribution name: lowercase, hyphens.
    let pyproject = find(&files, "pyproject.toml");
    assert!(pyproject.contents.contains("name = \"my-cool-contract-\""));

    // PEP 8 module name: lowercase, underscores, importable.
    assert!(files
        .iter()
        .any(|f| f.relative_path == "my_cool_contract_/__init__.py"));
    assert!(files
        .iter()
        .any(|f| f.relative_path == "my_cool_contract_/client.py"));
}

#[test]
fn falls_back_to_a_default_name_when_input_has_no_alphanumeric_characters() {
    let metadata = complex_metadata();
    let files = generate_python_package(&metadata, "!!!");

    assert!(files
        .iter()
        .any(|f| f.relative_path == "contract_client/__init__.py"));
}

#[test]
fn write_package_creates_an_installable_directory_layout() {
    let metadata = complex_metadata();
    let files = generate_python_package(&metadata, "sample_contract");

    let temp_dir = tempfile::tempdir().unwrap();
    write_package(temp_dir.path(), &files).unwrap();

    assert!(temp_dir.path().join("pyproject.toml").is_file());
    assert!(temp_dir.path().join("README.md").is_file());
    assert!(temp_dir
        .path()
        .join("sample_contract")
        .join("__init__.py")
        .is_file());
    assert!(temp_dir
        .path()
        .join("sample_contract")
        .join("client.py")
        .is_file());

    let written_client =
        std::fs::read_to_string(temp_dir.path().join("sample_contract").join("client.py")).unwrap();
    assert!(written_client.contains("class ContractClient"));
}

#[test]
fn readme_documents_pip_install_and_a_minimal_invoke_example() {
    let metadata = complex_metadata();
    let files = generate_python_package(&metadata, "sample_contract");

    let readme = find(&files, "README.md");
    assert!(readme.contents.contains("pip install ."));
    assert!(readme.contents.contains("from sample_contract import"));
}
