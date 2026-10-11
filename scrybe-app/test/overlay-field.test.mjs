// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import { validateField, trimOffer, scalarDisplay } from '../overlay/field.ts';
test('W9 limits and forbidden characters; bare ampersand and greater-than allowed', () => {
  for (const value of ['x\ny', '<tag>', '{{x}}', '{% x %}', 'x---y', '&copy;', '\u0085', ' edge ', 'x\u2028y']) assert.ok(validateField('title', value).length, value);
  assert.deepEqual(validateField('title', 'R&D > research'), []);
  assert.ok(validateField('title', '').length); assert.ok(validateField('title', 'x'.repeat(201)).length);
  assert.ok(validateField('dek', 'x'.repeat(301)).length); assert.ok(validateField('description', 'x'.repeat(1001)).length);
  assert.deepEqual(validateField('layout', 'doc'), []); // Editability comes from the server.
});
test('trim is offered explicitly and display respects scalar style', () => {
  assert.equal(trimOffer(' word '), 'word'); assert.equal(trimOffer('word'), null);
  assert.equal(scalarDisplay('Hello', 'plain'), 'Hello');
  assert.equal(scalarDisplay("It's good", 'single'), "'It''s good'");
  assert.equal(scalarDisplay('yes', 'plain'), '"yes"');
  assert.equal(scalarDisplay('a"b', 'double'), '"a\\"b"');
});

test('nested fields use the last key segment for length limits', () => {
  assert.deepEqual(validateField('card.title', 'x'.repeat(200)), []);
  assert.ok(validateField('card.title', 'x'.repeat(201)).length);
  assert.ok(validateField('card.title', '').length);
  assert.deepEqual(validateField('copy.card.heading', 'x'.repeat(300)), []);
  assert.ok(validateField('copy.card.heading', 'x'.repeat(301)).length);
  assert.deepEqual(validateField('card.description', 'x'.repeat(1000)), []);
  assert.ok(validateField('card.description', 'x'.repeat(1001)).length);
});
