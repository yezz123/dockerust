<div align="center">
    <a href="https://github.com/yezz123/dockerust" target="_blank">
        <img src="https://github.com/yezz123/dockerust/assets/52716203/740a385c-8eb9-46ec-811d-562e68668bcd">
    </a>
</div>

<p align="center">
    <a href="https://github.com/yezz123/dockerust/actions/workflows/ci.yml" target="_blank">
        <img src="https://github.com/yezz123/dockerust/actions/workflows/ci.yml/badge.svg" alt="CI Build Status">
    </a>
    <a href="https://codecov.io/gh/yezz123/dockerust">
        <img src="https://codecov.io/gh/yezz123/dockerust/branch/main/graph/badge.svg" alt="Code Coverage">
    </a>
    <a href="https://github.com/yezz123/dockerust/blob/main/LICENSE">
        <img src="https://img.shields.io/github/license/yezz123/dockerust.svg" alt="License">
    </a>
    <a href="https://github.com/yezz123/dockerust">
        <img src="https://img.shields.io/github/repo-size/yezz123/dockerust" alt="Repository Size">
    </a>
</p>

# Dockerust

Dockerust is an ambitious project aimed at building a fully-functional Docker registry server using the power and versatility of the Rust programming language.

## Project Overview

The primary goal of Dockerust is to create a robust Docker registry server that adheres to the official Docker Registry API specifications, as outlined in the [Docker Registry API documentation](https://docs.docker.com/registry/spec/api).

### Current State

As of now, Dockerust supports read operations, and the project is actively working towards implementing write features. This ongoing effort is geared towards unlocking the full potential of the registry, making it a reliable and production-ready solution.

## Getting Started

### Compilation

To compile Dockerust, ensure Rust is installed and run the following command:

```bash
cargo build --release
```

### Installation

Initialize the configuration by running:

```bash
dockerust init-config [conf_path]
```

To enable authentication and add user credentials:

```bash
dockerust add_user [conf_path]
```

Start Dockerust in server mode:

```bash
dockerust serve [conf_path]
```

## License

Dockerust is licensed under the [MIT License](https://github.com/yezz123/dockerust/blob/main/LICENSE). Feel free to explore, use, and contribute to this exciting project!
