mod bm25;
mod error;
mod filter;
mod index;
mod metadata;
mod metadata_index;
mod scoring;
mod types;

pub use bm25::Bm25Config;
pub use error::{Result, VectorKitError};
pub use filter::Filter;
pub use index::ExactVectorIndex;
pub use metadata::{Metadata, MetadataValue};
#[doc(hidden)]
pub use scoring::dot_product_i8 as diagnostic_dot_product_i8;
pub use types::{
    Chunk, ChunkId, ChunkInput, Document, IndexConfig, IndexFileSizeReport,
    IndexPersistenceOptions, IndexSizeEstimate, KeywordHit, KeywordQuery, SearchHit, SearchQuery,
    SearchTrace, StoredChunk, VectorEncoding, VectorMetric,
};
