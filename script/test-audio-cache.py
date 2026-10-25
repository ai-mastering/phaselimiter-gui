#!/usr/bin/env python3
"""Cache invalidation contracts: reuse independent builds, reject ABI changes."""
import copy
from pathlib import Path
import runpy
import unittest

from audio_common import PINS

cache = runpy.run_path(str(Path(__file__).with_name('audio-cache.py')))


class BuildKeys(unittest.TestCase):
    def setUp(self):
        self.pins = copy.deepcopy(PINS)
        self.recipes = {name: 'recipe-v1' for names in cache['RECIPES'].values() for name in names}
        self.tools = {'os': 'Darwin', 'arch': 'arm64', 'compiler': 'clang-16',
                      'sdk': '15.2', 'flags': {'deployment_target': '14.0'}}

    def keys(self, target='aarch64-apple-darwin'):
        return {name: cache['build_key'](name, target, self.pins, self.recipes, self.tools)
                for name in cache['SOURCES']}

    def test_backend_revision_does_not_rebuild_tools_or_libraries(self):
        before = self.keys()
        self.pins['backend']['revision'] = '0' * 40
        self.assertEqual(before, self.keys())

    def test_ffmpeg_source_change_does_not_rebuild_backend_libraries(self):
        before = self.keys()
        self.pins['sources']['ffmpeg']['sha256'] = 'new-source'
        after = self.keys()
        self.assertNotEqual(before['ffmpeg'], after['ffmpeg'])
        self.assertEqual(before['dependencies'], after['dependencies'])

    def test_boost_source_change_does_not_rebuild_ffmpeg(self):
        before = self.keys()
        self.pins['sources']['boost']['sha256'] = 'new-source'
        after = self.keys()
        self.assertEqual(before['ffmpeg'], after['ffmpeg'])
        self.assertNotEqual(before['dependencies'], after['dependencies'])

    def test_shared_zlib_change_rebuilds_both_components(self):
        before = self.keys()
        self.pins['sources']['zlib']['sha256'] = 'new-source'
        for name, value in self.keys().items():
            self.assertNotEqual(before[name], value)

    def test_recipe_changes_invalidate_only_affected_component(self):
        before = self.keys()
        self.recipes['script/build-ffmpeg.sh'] = 'recipe-v2'
        after = self.keys()
        self.assertNotEqual(before['ffmpeg'], after['ffmpeg'])
        self.assertEqual(before['dependencies'], after['dependencies'])
        self.recipes['script/audio_common.py'] = 'shared-recipe-v2'
        self.assertNotEqual(after['dependencies'], self.keys()['dependencies'])

    def test_compiler_sdk_and_flags_changes_invalidate_binaries(self):
        for field, value in [('compiler', 'clang-17'), ('sdk', '15.5'),
                             ('flags', {'deployment_target': '15.0'})]:
            with self.subTest(field=field):
                before = self.keys()
                self.tools[field] = value
                for name, result in self.keys().items():
                    self.assertNotEqual(before[name], result)

    def test_architectures_are_separate(self):
        before = self.keys()
        after = self.keys('x86_64-apple-darwin')
        for name in before:
            self.assertNotEqual(before[name], after[name])

    def test_mapping_order_does_not_affect_keys(self):
        before = self.keys()
        self.tools = dict(reversed(list(self.tools.items())))
        self.pins['sources'] = dict(reversed(list(self.pins['sources'].items())))
        self.assertEqual(before, self.keys())


if __name__ == '__main__':
    unittest.main()
