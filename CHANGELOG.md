# Changelog

## [0.3.0](https://github.com/kristoferssolo/sqrub/releases/tag/v0.3.0) - 2026-11-27

### Features

- Add criterion benchmarks for rewriting ([f8335b9](https://github.com/kristoferssolo/sqrub/commit/f8335b961e9b0ee754db81d8c1fa15b05e9a5c24))
- **cli:** Add --jobs to limit concurrent formatters ([d8662b5](https://github.com/kristoferssolo/sqrub/commit/d8662b50b0606155a33fd2398fe6d78f787bd2b2))

### Bug fixes

- **formatter:** Report stdin writer thread failures instead of panicking ([adc99fa](https://github.com/kristoferssolo/sqrub/commit/adc99fadc9fa186c5ce70094a765a74c16b257fb))
- **formatter:** Stop the formatter when its stdin writer cannot start ([3bdba80](https://github.com/kristoferssolo/sqrub/commit/3bdba8021243e4b1acaefb7cbc9807383405e0f5))
- Raise the open file limit for parallel formatters ([aa32ca7](https://github.com/kristoferssolo/sqrub/commit/aa32ca77283579ff255824a8d7e6fee486acf77d))

### Performance

- **rewrite:** Run formatter commands in parallel ([4954e47](https://github.com/kristoferssolo/sqrub/commit/4954e47fa59deaf576638509e31d1656479d5022))

### Refactoring

- Replace else-if chains with early returns ([bc64357](https://github.com/kristoferssolo/sqrub/commit/bc64357effca2b439bb756a834d98dc30bcc13fd))

### Documentation

- **readme:** Explain running formatters one at a time ([bfcc3b1](https://github.com/kristoferssolo/sqrub/commit/bfcc3b1a77c8844ba3f1f49723e5793814c51599))

**Full changelog:** https://github.com/kristoferssolo/sqrub/compare/v0.2.0...v0.3.0

## [0.2.0](https://github.com/kristoferssolo/sqrub/releases/tag/v0.2.0) - 2026-11-27

### Features

- **literal:** Render formatted sql as rust string literals ([cdd8634](https://github.com/kristoferssolo/sqrub/commit/cdd8634d5ed35b4ccf60fd79597ffd4453468106))
- **literal:** Decode literals into sql for the formatter ([571f703](https://github.com/kristoferssolo/sqrub/commit/571f70382c6f8373fd246fed7d1f25f8d7038c04))
- **extract:** Find sql literals in sqlx query macros ([8ee9196](https://github.com/kristoferssolo/sqrub/commit/8ee91960b1f4e88a03e68ce2e957a828171212e2))
- **formatter:** Pipe sql through an external formatter command ([1144ad3](https://github.com/kristoferssolo/sqrub/commit/1144ad37bc907fef40dd28f3139ed936f25947ec))
- **files:** Find rust files while honoring ignore files ([1cf5299](https://github.com/kristoferssolo/sqrub/commit/1cf5299d3cf52c034ec1e0c0d5e111326abeed91))
- **rewrite:** Format every sql literal in a source file ([89848c1](https://github.com/kristoferssolo/sqrub/commit/89848c1ca8dd56c1a3405cf52f9156d19506c87f))
- **cli:** Format files from the command line ([cc2913a](https://github.com/kristoferssolo/sqrub/commit/cc2913a2ff2691febfdeadd6d6212ea842538b59))

### Bug fixes

- **extract:** Indent multi-line literals where rustfmt places them ([2e49870](https://github.com/kristoferssolo/sqrub/commit/2e498702ee63662dcabc6986f9a7d53c9860ba71))

### Documentation

- Add readme ([3dc8f83](https://github.com/kristoferssolo/sqrub/commit/3dc8f83588135f2450e007b515906c51108d3283))

