# Dolin

A Rust-based archive engine with structured JSON report APIs, native FFI exports, CLI support, and JNI bindings for Android integration.

Dolin is designed as a clean native archive core that can evolve into:
- a standalone CLI tool
- a reusable Rust library
- an Android-native archive backend
- a foundation for a larger file/archive manager

---

## Vision

Dolin aims to provide a clean, maintainable, and extensible archive engine with a strong separation between:

- **core archive logic**
- **report and serialization layer**
- **foreign function interface**
- **platform integration layer**
- **application-level UI integration**

The long-term goal is to keep the archive engine independent, testable, and reusable across multiple environments.

---

## Current Capabilities

At the current stage, Dolin supports the following archive workflows:

- inspect archive information
- validate archive structure
- list archive entries
- extract a single entry
- extract a full archive
- pack a file or directory into an archive
- return structured JSON reports for integrations
- expose native functionality through JNI for Android use

---

## High-Level Architecture

```text
+----------------------+
|      Android UI      |
|  (future / external) |
+----------+-----------+
           |
           v
+----------------------+
|    Kotlin Bridge     |
|  parser + wrappers   |
+----------+-----------+
           |
           v
+----------------------+
|         JNI          |
|   thin string bridge |
+----------+-----------+
           |
           v
+----------------------+
|   Rust JSON APIs     |
| report-based methods |
+----------+-----------+
           |
           v
+----------------------+
|   Rust Core Engine   |
| read / write / pack  |
+----------------------+
