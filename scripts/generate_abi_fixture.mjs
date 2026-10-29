import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const [strucppRoot, output, source] = process.argv.slice(2);
if (!strucppRoot || !output || process.argv.length > 5) {
  throw new Error(
    "usage: generate_abi_fixture.mjs <strucpp-root> <output-dir> [source.st]",
  );
}

const root = resolve(strucppRoot);
const out = resolve(output);
const fixture = source
  ? resolve(source)
  : resolve(
      dirname(fileURLToPath(import.meta.url)),
      "../fixtures/abi/located-cycle.st",
    );
const { compile } = await import(
  pathToFileURL(join(root, "dist/index.js")).href
);
const result = compile(readFileSync(fixture, "utf8"), {
  debug: true,
  fileName: basename(fixture),
  headerFileName: "generated.hpp",
});
if (!result.success || !result.headerCode || !result.debugTableCpp) {
  throw new Error(JSON.stringify(result.errors));
}

mkdirSync(out, { recursive: true });
writeFileSync(join(out, "generated.cpp"), result.cppCode);
writeFileSync(join(out, "generated.hpp"), result.headerCode);
writeFileSync(join(out, "generated_debug.cpp"), result.debugTableCpp);
writeFileSync(join(out, "debug-map.json"), JSON.stringify(result.debugMap, null, 2));
writeFileSync(
  join(out, "defines.h"),
  '#define PROGRAM_MD5 "0123456789abcdef0123456789abcdef"\n',
);
