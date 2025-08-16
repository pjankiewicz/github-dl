# github-dl

A simple command-line tool to download a specific folder from a GitHub repository.

## Features

- Download any folder from a public or private GitHub repository.
- Refresh previously downloaded folders to pull the latest changes.
- Parallel downloads for faster performance.
- Respects `.env` files for GitHub API token authentication.

## Installation

You can install `github-dl` directly from crates.io using Cargo:

```sh
cargo install github-dl
```

## Usage

### Download a Folder

To download a folder, use the `download` command with the GitHub folder URL and an output directory.

```sh
github-dl download <GITHUB_URL> -o <OUTPUT_DIRECTORY>
```

**Example:**

```sh
github-dl download "https://github.com/rust-lang/book/tree/main/src/ch01-00-introduction" -o ./rust-book-intro
```

This will download the contents of the `ch01-00-introduction` folder into the `./rust-book-intro` directory.

### Refresh a Folder

The tool creates a hidden `.github-dl.json` metadata file in the output directory. This allows you to easily refresh the folder's contents later.

To refresh all downloaded folders in the current directory (or a specified base directory), use the `refresh` command.

```sh
# Refresh folders in the current directory
github-dl refresh

# Refresh folders in a specific base directory
github-dl refresh --base-dir /path/to/projects
```

The `refresh` command will scan for `.github-dl.json` files recursively and update the contents of each folder from its original GitHub source.

### Parallel Jobs

You can control the number of parallel downloads using the `-j` or `--jobs` flag. The default is 5.

```sh
github-dl download <URL> -o <OUTPUT> -j 10
```

## Authentication

For private repositories or to avoid hitting GitHub's API rate limits, you can provide a personal access token.

Create a `.env` file in the directory where you run the command (or in any parent directory) with the following content:

```
GITHUB_TOKEN=your_personal_access_token_here
```

`github-dl` will automatically detect and use this token for all API requests.

## License

This project is licensed under either of
* Apache License, Version 2.0
* MIT license

at your option.