for model in \
  bge-small-en-v1.5 \
  all-MiniLM-L6-v2 \
  e5-small-v2 \
  gte-small \
  snowflake-arctic-embed-xs \
  snowflake-arctic-embed-s
do
  .build/release/retrievalkit-bench \
    --real-index "../../../target/examples/social-network-48k-384d/$model/index" \
    --query-embeddings "../../../target/examples/social-network-48k-384d/$model/queries.json" \
    > "../../../target/examples/social-network-48k-384d/$model/swift-search-report.md"
done
