// SPDX-License-Identifier: Apache-2.0
// The lazily loaded WYSIWYG bundle: the Milkdown surface plus its styles.
// Kept apart from milkdown-surface.ts so that module stays importable in
// Node tests, which cannot load CSS.
import "@milkdown/crepe/theme/common/style.css";
import "@milkdown/crepe/theme/frame.css";
import "./styles/wysiwyg.css";

export { createMilkdownSurface } from "./milkdown-surface";
