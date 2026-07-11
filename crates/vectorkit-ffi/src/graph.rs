/// ABI version for the optional aggregate core + graph artifact.
///
/// Its presence proves the library was built with the `graph` feature. The
/// aggregate is selected instead of the base FFI artifact, so both retrieval
/// and graph entry points share one linked Rust core implementation.
#[no_mangle]
pub extern "C" fn vectorkit_graph_ffi_abi_version() -> u32 {
    let _ = std::mem::size_of::<vectorkit_graph::GraphIndex>();
    1
}

#[cfg(test)]
mod tests {
    #[test]
    fn aggregate_abi_version_is_stable() {
        assert_eq!(super::vectorkit_graph_ffi_abi_version(), 1);
    }
}
