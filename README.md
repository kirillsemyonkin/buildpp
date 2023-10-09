# Build++

Build++ is a project builder that can simplify building projects in multitude of compilers and
environments.

## Installation

[Installing Rust](https://www.rust-lang.org/tools/install) is required to install this application.

Then, you may install (and update) directly from this repository using following command:

```sh
cargo install --git https://github.com/kirillsemyonkin/buildpp
```

If installed successfully, with a proper Rust setup, it should automatically appear in your `PATH`,
you can check it using the following command:

```sh
buildpp --version
```

If not working, try selecting installation path using Cargo's `--root ...` flag and adding that to
the `PATH` environment variable manually.

## Usage

Build++ requires a `build++.lsd` configuration and `src` directory. As projects build, they generate
`cache` (dependencies) and `target` (output) directories, with `<version>/<profile>` structure.

Source directory has to contain either `main.cpp` file for `binary` projects and / or `lib.cpp` for
static or shared library projects. This file will have to include all headers in your project.

> *Specifying custom entry source file may be added in future,
but then explicit build type (`--is` or configuration variable) may be required.*

### Configuration

Every project should contain a `build++.lsd` configuration file in the LSD file format (*TBD*).
It consists of 4 sections (also see `example.build++.lsd`):

- Metadata, like `name` and `version`, optionally `run` command.
- Dependencies, be it other Build++ projects, compiled libraries or something else.
- Profiles that select which compiler to use and what flags to pass to it.

---

#### Metadata

```lsd
name project-name      # required
version 0.1.0          # required, `0.1.0` is done by `buildpp new`

run mpiexec -n 16 {}   # optional,
                       # use `{}` to substitute executable file path
                       # default value is `{}`

# also:
run [ mpiexec -n 16 {} ]
run {
    command mpiexec
    arguments [ -n 16 {} ]
}
```

---

#### Dependencies

Every dependency is located in the `dependency` level, and has an `is` value to determine its type.
Its name has to be unique and it is used only internally and to document your project (it does not
have to match any other name). Dependencies are entirely optional.

Currently supported types:

- Build++ project: `local build++` (`local build`, `local buildpp`, `local`).

```lsd
dependency {
    other-project {
        is local build++
        path ../other-project   # required
        profile inherit         # optional,
                                # `inherit` will copy currently selected running profile,
                                # the profile `default` is used otherwise
    }
}
```

- Compiled library: `local pair` (`local include`, `local library`).

```lsd
dependency {
    msmpi {
        is local pair
        include C:/Program Files (x86)/Microsoft SDKs/MPI/Include   # required
        library C:/Program Files (x86)/Microsoft SDKs/MPI/Lib/x64   # required
    }
}
```

Types that will be added in future:

- [ ] Git-stored build++ project: `git build++` (`git buildpp`, `git build`, `git`).
- [ ] Downloadable compiled library: `remote pair` (`remote include`, `remote library`).
- Also add more optional values may be added in the future to current types.

---

#### Profiles

Every profile is stored in `profile` level, and has an `is` value to determine its type. Its name is
unique and is optionally used in commands and during dependencies inheriting profiles. Default
profile is named `default`. Unlike dependencies, profiles are required to determine compiler.

Profiles may be copied and then extended with extra values using the `inherit` option:

```lsd
profile {
    default {
        is msvc
    }
    release {
        inherit default   # release also `is msvc`
    }
}
```

Currently supported types:

> ⚠️ Library type `static` is currently not properly supported.

- MSVC: `msvc`.

> ⚠️ If MSVC complains with `No such file or directory` on one of the standard library's headers,
    try running [`vcvars64`](https://learn.microsoft.com/en-us/cpp/build/building-on-the-command-line?view=msvc-170#developer_command_file_locations)
    in the current command prompt before using Build++. Not all shells support `vcvars64`.

```lsd
profile {
    default {
        is msvc

        compiler_path cl   # optional, default: `cl`
        standard c++20     # optional, supported:
                           # `c++14`, `c++17`, `c++20`, `c++latest`, `c11`, `c17`
        optimize speed     # optional, supported: `O1` (`size`), `O2` (`speed`)
        openmp true        # optional, enables OpenMP pragmas
        library shared     # optional, supported: `shared` (default), `static`
    }
}
```

- CUDA: `nvcc` (`cuda`).

```lsd
profile {
    default {
        is nvcc
        
        compiler_path nvcc   # optional, default: `nvcc`
        standard c++20       # optional, supported: `c++03`, `c++11`, `c++14`, `c++17`, `c++20`
        optimize O3          # optional, look up GNU or NVCC optimization levels
                             # or `src/profile/nvcc.rs` Build++ source code file
        dopt true            # optional, enables `dopt` flag
        library shared       # optional, supported: `shared` (default), `static`
    }
}
```

Types that will be added in future:

- [ ] GNU C++: `gnu` (`g++`).
- [ ] GNU C: `gcc`.
- Also add more optional values may be added in the future to current types.

### Commands

Use following commands to use Build++ (flags may start with `--`, `-` or `/`):

```sh
buildpp --version       # show version
buildpp --help          # show help

buildpp build           # build the project into `target` directory
    --is library        # optional: will detect from either `main.cpp` or `lib.cpp`.
                        # also profiles may override this detection, like CUDA's `main.cu`.
    --profile default   # optional, profile name (default is `default`)

buildpp run             # build and run the program
    -- args...          # optional, additional arguments to pass to the running program
    --profile default   # optional, profile name (default is `default`)

buildpp new             # create project with `build++.lsd` and `src` hello world program
    --is binary         # required, supported: `binary` (main.cpp) or `library` (lib.cpp)
    --name project-name # required, name of the folder / metadata to be made
```

> ⚠️ `--help` is currently not properly supported due to incomplete command architecture (registries
  of currently available commands, dependency and profile types have to be added).

Future command features:

- [ ] Git / `.gitignore` in project creation.
- [ ] Initialization of existing projects (by adding `build++.lsd`).
- [ ] Clearing `cache` and, optionally, `target`.
- [ ] Managing configuration from command line (adding dependencies, profiles).
- [ ] Installing programs.
- [ ] Support `--help` properly.

## Future features

- Complete all features in every block in this document.
- [ ] Make LSD be own project.
- [ ] Add support for other languages (at least as libraries).
- [ ] Currently properly supported for Windows, make everything work for Linux.
- [ ] Add tests.
