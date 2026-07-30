export const MODEL_ID = "sentence-transformers/all-MiniLM-L6-v2";
export const SOURCE_MODEL_REVISION =
  "c9745ed1d9f207416be6d2e6f8de32d1f16199bf";
export const ARTIFACT_REPOSITORY = "gungorbasa/retrievalkit-minilm";
export const ARTIFACT_REVISION =
  "617ce926c1f9e0289365d3e999474cc28b1645d4";
export const ARTIFACT_MANIFEST_SHA256 =
  "b81e0e9393a25630eda184cfa373f2f28eed08c2ed92ae3d4097504e5f7ab4b2";
export const EMBEDDING_DIMENSION = 384;
export const MAX_INPUT_TOKENS = 256;
export const CACHE_SCHEMA = "retrievalkit-browser-embedding-v1";

export interface ArtifactSpec {
  readonly path: string;
  readonly bytes: number;
  readonly sha256: string;
  readonly url: string;
}

const artifactBase =
  `https://huggingface.co/${ARTIFACT_REPOSITORY}/resolve/${ARTIFACT_REVISION}`;

function artifact(path: string, bytes: number, sha256: string): ArtifactSpec {
  return {
    path,
    bytes,
    sha256,
    url: `${artifactBase}/${path}?download=true`
  };
}

export const PINNED_ARTIFACTS: readonly ArtifactSpec[] = Object.freeze([
  artifact("manifest-v1.json", 4_797, ARTIFACT_MANIFEST_SHA256),
  artifact(
    "onnx/all-MiniLM-L6-v2-fp32.onnx",
    90_396_663,
    "beaa83a6670eb0ddae4d7c6f7a89acf69ed5d1fd747b083fa6f9f0145b2ee891"
  ),
  artifact(
    "tokenizer/tokenizer.json",
    466_247,
    "be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037"
  ),
  artifact(
    "tokenizer/tokenizer_config.json",
    350,
    "acb92769e8195aabd29b7b2137a9e6d6e25c476a4f15aa4355c233426c61576b"
  ),
  artifact(
    "tokenizer/special_tokens_map.json",
    112,
    "303df45a03609e4ead04bc3dc1536d0ab19b5358db685b6f3da123d05ec200e3"
  ),
  artifact(
    "tokenizer/vocab.txt",
    231_508,
    "07eced375cec144d27c900241f3e339478dec958f92fddbc551f295c992038a3"
  )
]);

export const MODEL_INFO = Object.freeze({
  identifier: MODEL_ID,
  sourceRevision: SOURCE_MODEL_REVISION,
  artifactRepository: ARTIFACT_REPOSITORY,
  artifactRevision: ARTIFACT_REVISION,
  artifactManifestSha256: ARTIFACT_MANIFEST_SHA256,
  dimension: EMBEDDING_DIMENSION,
  maxInputTokens: MAX_INPUT_TOKENS,
  normalized: true as const,
  precision: "fp32" as const
});
