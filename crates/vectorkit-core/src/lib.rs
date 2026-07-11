mod bm25;
mod candidate_scope;
mod error;
mod filter;
mod index;
mod metadata;
mod metadata_index;
mod record_store;
mod scoring;
mod types;

pub use bm25::Bm25Config;
pub use candidate_scope::CandidateScope;
pub use error::{Result, VectorKitError};
pub use filter::Filter;
pub use index::ExactVectorIndex;
pub use metadata::{Metadata, MetadataValue};
pub use record_store::{
    ChunkIdentity, ChunkKey, CorpusId, FieldName, GenerationId, Record, RecordId, RecordStore,
    RecordType, RecordValue,
};
#[doc(hidden)]
pub use scoring::dot_product_i8 as diagnostic_dot_product_i8;
pub use types::{
    Chunk, ChunkId, ChunkInput, CompactionReport, Document, HybridFusion, HybridFusionTrace,
    HybridHit, HybridQuery, HybridTrace, IndexConfig, IndexFileSizeReport, IndexPersistenceOptions,
    IndexSizeEstimate, KeywordHit, KeywordQuery, RecordChunkInput, SearchHit, SearchQuery,
    SearchTrace, StoredChunk, VectorEncoding, VectorMetric,
};
