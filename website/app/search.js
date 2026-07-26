/**
 * @param {{
 *   eyebrow: string;
 *   title: string;
 *   summary: string;
 *   body: string;
 *   code?: string;
 *   quickstart?: string;
 *   expected?: string;
 *   tags?: string[];
 * }} section
 */
function searchableText(section) {
  return [
    section.eyebrow,
    section.title,
    section.summary,
    section.body,
    section.code ?? "",
    section.quickstart ?? "",
    section.expected ?? "",
    ...(section.tags ?? []),
  ]
    .join(" ")
    .toLowerCase();
}

/**
 * @param {Parameters<typeof searchableText>[0]} section
 * @param {string} query
 */
export function matchesDocumentationSection(section, query) {
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) {
    return true;
  }

  const haystack = searchableText(section);
  return terms.every((term) => haystack.includes(term));
}
