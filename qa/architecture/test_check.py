"""Contract tests for dependency guards, including legitimate test fixtures."""
from pathlib import Path
import tempfile
import unittest

from check import violations


class ArchitectureTests(unittest.TestCase):
    def inspect(self, path, code):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / path
            source.parent.mkdir(parents=True)
            source.write_text(code)
            findings, _ = violations(root)
            return {finding['rule'] for finding in findings}

    def test_services_cannot_call_http_handlers(self):
        for owner in ('player/graphics.rs', 'utils/config.rs', 'graphics/control/graphics.rs', 'graphics.rs'):
            self.assertEqual(self.inspect('backend/app/src/' + owner,
                                         'use crate::{api::routes::apply_scene};'),
                             {'service-depends-on-http'})

    def test_engine_cannot_import_application_dependencies(self):
        for dependency in ('noar_app', 'axum', 'sqlx'):
            self.assertEqual(self.inspect('backend/engine/src/frame.rs',
                                         f'use {dependency}::State;'),
                             {'engine-depends-on-application'})

    def test_production_queries_stay_in_the_data_layer(self):
        for query in ('sqlx::query("SELECT 1")', 'sqlx::query_as::<_, Model>("SELECT 1")',
                      'use sqlx::{SqlitePool, query_scalar};'):
            self.assertEqual(self.inspect('backend/app/src/utils/config.rs', query),
                             {'sql-outside-data-layer'})
            self.assertEqual(self.inspect('backend/app/src/db/handles/config.rs', query), set())

    def test_pool_types_and_test_fixtures_are_allowed(self):
        self.assertEqual(self.inspect('backend/app/src/utils/config.rs', '''
use sqlx::SqlitePool;
#[cfg(test)]
mod tests { sqlx::query("SELECT 1"); }
'''), set())

    def test_comments_and_strings_are_not_dependencies(self):
        self.assertEqual(self.inspect('backend/app/src/utils/config.rs', '''
//! Do not call api::routes here.
const EXAMPLE: &str = r#"sqlx::query("sample")"#;
/* api::routes::example(); */
'''), set())

    def test_production_after_a_test_module_is_still_checked(self):
        self.assertEqual(self.inspect('backend/app/src/graphics.rs', '''
#[cfg(test)]
mod tests { const BRACE: char = '{'; }
fn production() { crate::api::routes::apply_scene(); }
'''), {'service-depends-on-http'})

    def test_test_only_module_declarations_do_not_hide_later_queries(self):
        self.assertEqual(self.inspect('backend/app/src/utils/config.rs', '''
#[cfg(test)]
mod test_support;
fn production() { sqlx::query("SELECT 1"); }
'''), {'sql-outside-data-layer'})


    def test_presentational_panels_do_not_own_stores_or_http(self):
        path = 'frontend/src/components/storage/StorageImageGrid.vue'
        for code in (
            "import { useConfig } from '@/stores/config'",
            "import { authFetch } from '@/composables/authFetch'",
        ):
            self.assertEqual(self.inspect(path, code), {'panel-depends-on-data-owner'})
        self.assertEqual(self.inspect(path, 'fetch(url)'), {'panel-opens-network'})

    def test_panels_allow_typed_commands_and_ignore_network_examples(self):
        path = 'frontend/src/components/player/playlist/PlaylistDayDialogs.vue'
        self.assertEqual(self.inspect(path, """
// Do not import { authFetch } from '@/composables/authFetch'
const hint = 'fetch(url)';
const emit = defineEmits<{ save: [] }>();
"""), set())
        self.assertEqual(self.inspect('frontend/src/composables/reportData.ts',
                                      "import { authFetch } from '@/composables/authFetch'"), set())


    def toolchain(self, pinned, image, ci):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / '.github/workflows').mkdir(parents=True)
            (root / 'rust-toolchain.toml').write_text(f'[toolchain]\nchannel = "{pinned}"\n')
            (root / 'Dockerfile').write_text(f'ARG NODE_IMAGE=node:24\nARG RUST_IMAGE=rust:{image}-slim-trixie\n')
            (root / '.github/workflows/validate.yml').write_text(f'jobs:\n  build:\n    container: rust:{ci}-trixie\n')
            findings, _ = violations(root)
            return [(finding['rule'], finding['path']) for finding in findings]

    def test_local_builds_ci_and_the_image_use_one_rust(self):
        self.assertEqual(self.toolchain('1.99.1', '1.99.1', '1.99.1'), [])
        # Bumped in one place only: Clippy would differ between them.
        self.assertEqual(self.toolchain('1.99.1', '1.100.0', '1.99.1'),
                         [('rust-toolchain-mismatch', 'Dockerfile')])
        self.assertEqual(self.toolchain('1.100.0', '1.99.1', '1.99.1'),
                         [('rust-toolchain-mismatch', 'Dockerfile'),
                          ('rust-toolchain-mismatch', '.github/workflows/validate.yml')])

    def test_the_repository_pins_one_rust(self):
        findings, _ = violations(Path(__file__).resolve().parents[2])
        self.assertEqual([finding for finding in findings if finding['rule'].startswith('rust-toolchain')], [])


if __name__ == '__main__':
    unittest.main()
