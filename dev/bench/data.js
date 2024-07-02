window.BENCHMARK_DATA = {
  "lastUpdate": 1719931598146,
  "repoUrl": "https://github.com/bosun-ai/swiftide",
  "entries": {
    "Rust Benchmark": [
      {
        "commit": {
          "author": {
            "email": "29139614+renovate[bot]@users.noreply.github.com",
            "name": "renovate[bot]",
            "username": "renovate[bot]"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5c16c8e8fd732588021e01c887ddde82deb8b982",
          "message": "fix(deps): update rust crate strum to v0.26.3 (#101)\n\n[![Mend\r\nRenovate](https://app.renovatebot.com/images/banner.svg)](https://renovatebot.com)\r\n\r\nThis PR contains the following updates:\r\n\r\n| Package | Type | Update | Change |\r\n|---|---|---|---|\r\n| [strum](https://togithub.com/Peternator7/strum) | dependencies | patch\r\n| `0.26.2` -> `0.26.3` |\r\n\r\n---\r\n\r\n### Release Notes\r\n\r\n<details>\r\n<summary>Peternator7/strum (strum)</summary>\r\n\r\n###\r\n[`v0.26.3`](https://togithub.com/Peternator7/strum/blob/HEAD/CHANGELOG.md#0263-strummacros)\r\n\r\n[Compare\r\nSource](https://togithub.com/Peternator7/strum/compare/v0.26.2...v0.26.3)\r\n\r\n- [#&#8203;344](https://togithub.com/Peternator7/strum/pull/344): Hide\r\n`EnumTable` because it's going to be deprecated in the next\r\n    version.\r\n- [#&#8203;357](https://togithub.com/Peternator7/strum/pull/357): Fixes\r\nan incompatiblity with `itertools` by using the fully\r\n    qualified name rather than the inherent method.\r\n- [#&#8203;345](https://togithub.com/Peternator7/strum/pull/345): Allows\r\nunnamed tuple like variants to use their variants in\r\nstring interpolation. `#[strum(to_string = \"Field 0: {0}, Field 1:\r\n{1})\")]` will now work for tuple variants\r\n\r\n</details>\r\n\r\n---\r\n\r\n### Configuration\r\n\r\n📅 **Schedule**: Branch creation - At any time (no schedule defined),\r\nAutomerge - At any time (no schedule defined).\r\n\r\n🚦 **Automerge**: Disabled by config. Please merge this manually once you\r\nare satisfied.\r\n\r\n♻ **Rebasing**: Whenever PR becomes conflicted, or you tick the\r\nrebase/retry checkbox.\r\n\r\n🔕 **Ignore**: Close this PR and you won't be reminded about this update\r\nagain.\r\n\r\n---\r\n\r\n- [ ] <!-- rebase-check -->If you want to rebase/retry this PR, check\r\nthis box\r\n\r\n---\r\n\r\nThis PR has been generated by [Mend\r\nRenovate](https://www.mend.io/free-developer-tools/renovate/). View\r\nrepository job log\r\n[here](https://developer.mend.io/github/bosun-ai/swiftide).\r\n\r\n<!--renovate-debug:eyJjcmVhdGVkSW5WZXIiOiIzNy40MjAuMSIsInVwZGF0ZWRJblZlciI6IjM3LjQyMC4xIiwidGFyZ2V0QnJhbmNoIjoibWFzdGVyIiwibGFiZWxzIjpbXX0=-->\r\n\r\nCo-authored-by: renovate[bot] <29139614+renovate[bot]@users.noreply.github.com>",
          "timestamp": "2024-06-28T17:12:58+02:00",
          "tree_id": "eb36e9c783ec680f318560c7e53855f7474652c9",
          "url": "https://github.com/bosun-ai/swiftide/commit/5c16c8e8fd732588021e01c887ddde82deb8b982"
        },
        "date": 1719587822385,
        "tool": "cargo",
        "benches": [
          {
            "name": "load_1",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "load_10",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "run_local_pipeline",
            "value": 837,
            "range": "± 4",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "timonv@gmail.com",
            "name": "Timon Vonk",
            "username": "timonv"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "929410cb1c2d81b6ffaec4c948c891472835429d",
          "message": "docs(readme): add diagram to the readme (#107)",
          "timestamp": "2024-06-29T22:47:44+02:00",
          "tree_id": "99e2fc152be373ae77bff10332a77589f31907db",
          "url": "https://github.com/bosun-ai/swiftide/commit/929410cb1c2d81b6ffaec4c948c891472835429d"
        },
        "date": 1719694296634,
        "tool": "cargo",
        "benches": [
          {
            "name": "load_1",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "load_10",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "run_local_pipeline",
            "value": 837,
            "range": "± 29",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "timonv@gmail.com",
            "name": "Timon Vonk",
            "username": "timonv"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6a88651df8c6b91add03acfc071fb9479545b8af",
          "message": "feat(ingestion_pipeline): implement filter (#109)",
          "timestamp": "2024-06-30T23:57:58+02:00",
          "tree_id": "f00872417dae4ea6faa90f1928aa0af9695bba91",
          "url": "https://github.com/bosun-ai/swiftide/commit/6a88651df8c6b91add03acfc071fb9479545b8af"
        },
        "date": 1719784898765,
        "tool": "cargo",
        "benches": [
          {
            "name": "load_1",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "load_10",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "run_local_pipeline",
            "value": 837,
            "range": "± 2",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "timonv@gmail.com",
            "name": "Timon Vonk",
            "username": "timonv"
          },
          "committer": {
            "email": "mail@timonv.nl",
            "name": "Timon Vonk",
            "username": "timonv"
          },
          "distinct": true,
          "id": "a12cce230032eebe2f7ff1aa9cdc85b8fc200eb1",
          "message": "fix(openai): add tests for builder",
          "timestamp": "2024-07-01T00:15:11+02:00",
          "tree_id": "4d51f3f7e0532cecac98f8a82c9c4946e8e3c487",
          "url": "https://github.com/bosun-ai/swiftide/commit/a12cce230032eebe2f7ff1aa9cdc85b8fc200eb1"
        },
        "date": 1719785958253,
        "tool": "cargo",
        "benches": [
          {
            "name": "load_1",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "load_10",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "run_local_pipeline",
            "value": 836,
            "range": "± 1",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "timonv@gmail.com",
            "name": "Timon Vonk",
            "username": "timonv"
          },
          "committer": {
            "email": "timonv@gmail.com",
            "name": "Timon Vonk",
            "username": "timonv"
          },
          "distinct": true,
          "id": "162c6ef2a07e40b8607b0ab6773909521f0bb798",
          "message": "chore: ensure feat is always in Added",
          "timestamp": "2024-07-01T00:29:13+02:00",
          "tree_id": "40ce2cdb8819bfb1f1897e6bd9c35527602e29cf",
          "url": "https://github.com/bosun-ai/swiftide/commit/162c6ef2a07e40b8607b0ab6773909521f0bb798"
        },
        "date": 1719786788931,
        "tool": "cargo",
        "benches": [
          {
            "name": "load_1",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "load_10",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "run_local_pipeline",
            "value": 837,
            "range": "± 1",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "timonv@gmail.com",
            "name": "Timon Vonk",
            "username": "timonv"
          },
          "committer": {
            "email": "timonv@gmail.com",
            "name": "Timon Vonk",
            "username": "timonv"
          },
          "distinct": true,
          "id": "17a2be1de6c0f3bda137501db4b1703f9ed0b1c5",
          "message": "fix(changelog): add scope",
          "timestamp": "2024-07-01T10:31:44+02:00",
          "tree_id": "5ce07b7c3e032e2c739f538802629e1a306cd877",
          "url": "https://github.com/bosun-ai/swiftide/commit/17a2be1de6c0f3bda137501db4b1703f9ed0b1c5"
        },
        "date": 1719822953249,
        "tool": "cargo",
        "benches": [
          {
            "name": "load_1",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "load_10",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "run_local_pipeline",
            "value": 837,
            "range": "± 47",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "timonv@gmail.com",
            "name": "Timon Vonk",
            "username": "timonv"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b014f43aa187881160245b4356f95afe2c6fe98c",
          "message": "docs: improve documentation across the project (#112)",
          "timestamp": "2024-07-01T12:27:24+02:00",
          "tree_id": "3fa56605fb2c63ebc0ca6a9313b862462d3296bd",
          "url": "https://github.com/bosun-ai/swiftide/commit/b014f43aa187881160245b4356f95afe2c6fe98c"
        },
        "date": 1719829870176,
        "tool": "cargo",
        "benches": [
          {
            "name": "load_1",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "load_10",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "run_local_pipeline",
            "value": 838,
            "range": "± 54",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "41898282+github-actions[bot]@users.noreply.github.com",
            "name": "github-actions[bot]",
            "username": "github-actions[bot]"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a8b02a3779bcce3671ea21d479fd4ceda30316c0",
          "message": "chore: release v0.5.0 (#103)\n\n## 🤖 New release\r\n* `swiftide`: 0.4.3 -> 0.5.0\r\n\r\n<details><summary><i><b>Changelog</b></i></summary><p>\r\n\r\n<blockquote>\r\n\r\n## [0.5.0] - 2024-07-01\r\n\r\n### Added\r\n\r\n- AWS bedrock support\r\n([#92](https://github.com/bosun-ai/swiftide/pull/92))\r\n- (readme): Add diagram to the readme\r\n([#107](https://github.com/bosun-ai/swiftide/pull/107))\r\n- (ingestion_pipeline): Implement filter\r\n([#109](https://github.com/bosun-ai/swiftide/pull/109))\r\n- (ingestion_pipeline): Splitting and merging streams\r\n- (ingestion_pipeline): Build a pipeline from a stream\r\n- (openai): Add tests for builder\r\n\r\n### Changed\r\n\r\n- (deps): Update rust crate testcontainers to 0.19.0\r\n([#102](https://github.com/bosun-ai/swiftide/pull/102))\r\n- Improve documentation across the project\r\n([#112](https://github.com/bosun-ai/swiftide/pull/112))\r\n\r\n### Fixed\r\n\r\n- Fix oversight in ingestion pipeline tests\r\n- (deps): Update rust crate text-splitter to 0.14.0\r\n([#105](https://github.com/bosun-ai/swiftide/pull/105))\r\n- Replace unwrap with expect and add comment on panic\r\n- (transformers): Fix too small chunks being retained and api\r\n\r\n[0.5.0]: https://github.com///compare/0.1.0..0.5.0\r\n\r\n<!-- generated by git-cliff -->\r\n</blockquote>\r\n\r\n\r\n</p></details>\r\n\r\n---\r\nThis PR was generated with\r\n[release-plz](https://github.com/MarcoIeni/release-plz/).\r\n\r\nCo-authored-by: github-actions[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2024-07-01T12:49:14+02:00",
          "tree_id": "6f1bdafc86bc7722bb49e4f2a0d914bb77bf5df8",
          "url": "https://github.com/bosun-ai/swiftide/commit/a8b02a3779bcce3671ea21d479fd4ceda30316c0"
        },
        "date": 1719831175657,
        "tool": "cargo",
        "benches": [
          {
            "name": "load_1",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "load_10",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "run_local_pipeline",
            "value": 837,
            "range": "± 4",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "timonv@gmail.com",
            "name": "Timon Vonk",
            "username": "timonv"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "353cd9ed36fcf6fb8f1db255d8b5f4a914ca8496",
          "message": "fix(qdrant): upgrade and better defaults (#118)\n\n- **fix(deps): update rust crate qdrant-client to v1.10.1**\r\n- **fix(qdrant): upgrade to new qdrant with sensible defaults**\r\n- **feat(qdrant): safe to clone with internal arc**\r\n\r\n---------\r\n\r\nCo-authored-by: renovate[bot] <29139614+renovate[bot]@users.noreply.github.com>",
          "timestamp": "2024-07-02T16:42:42+02:00",
          "tree_id": "13d47de24d4f2d8c6f4a61fe2a6ec3e100185e90",
          "url": "https://github.com/bosun-ai/swiftide/commit/353cd9ed36fcf6fb8f1db255d8b5f4a914ca8496"
        },
        "date": 1719931597710,
        "tool": "cargo",
        "benches": [
          {
            "name": "load_1",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "load_10",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "run_local_pipeline",
            "value": 836,
            "range": "± 34",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}