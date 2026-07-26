export const SUPPORTED_NODE_RANGES = Object.freeze([
  Object.freeze({ major: 22, minimum: Object.freeze([22, 13, 0]) }),
  Object.freeze({ major: 24, minimum: Object.freeze([24, 0, 0]) })
]);

export const NODE_ENGINES = "^22.13.0 || ^24.0.0";
export const RECOMMENDED_NODE_MAJOR = 24;

function parseVersion(version) {
  const match = /^(?:v)?(\d+)\.(\d+)\.(\d+)/u.exec(version);
  if (match === null) {
    return null;
  }

  return match.slice(1).map((component) => Number.parseInt(component, 10));
}

function isAtLeast(version, minimum) {
  for (let index = 0; index < minimum.length; index += 1) {
    if (version[index] > minimum[index]) {
      return true;
    }
    if (version[index] < minimum[index]) {
      return false;
    }
  }
  return true;
}

export function isSupportedNodeVersion(version) {
  const parsed = parseVersion(version);
  if (parsed === null) {
    return false;
  }

  const range = SUPPORTED_NODE_RANGES.find(({ major }) => major === parsed[0]);
  return range !== undefined && isAtLeast(parsed, range.minimum);
}

export function supportedNodeDescription() {
  return "Node.js 22.13+ LTS or Node.js 24 LTS";
}

export function unsupportedNodeMessage(version) {
  return `${supportedNodeDescription()} is required; detected Node.js ${version}. Install the latest Node.js ${RECOMMENDED_NODE_MAJOR} LTS release (recommended) or a supported Node.js 22 LTS release, then retry. If you use nvm, run 'nvm install ${RECOMMENDED_NODE_MAJOR} && nvm use ${RECOMMENDED_NODE_MAJOR}'.`;
}
