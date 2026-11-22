// Copyright 2026 Renato Cavalcanti
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The version this build carries.

/// The version baked in from `JAM_FACTORY_VERSION`, else the package version.
pub const BUILD_VERSION: &str = build_version(option_env!("JAM_FACTORY_VERSION"));

/// `env` when it is set and not empty, else the package version.
const fn build_version(env: Option<&'static str>) -> &'static str {
    match env {
        Some(version) if !version.is_empty() => version,
        _ => env!("CARGO_PKG_VERSION"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_build_version_falls_back_to_the_package_version() {
        assert_eq!(build_version(None), env!("CARGO_PKG_VERSION"));
        assert_eq!(build_version(Some("")), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn the_build_version_takes_the_given_version() {
        assert_eq!(build_version(Some("0.1.4-3e24a0f1")), "0.1.4-3e24a0f1");
    }
}
