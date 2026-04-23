/**
 * CatalogEmptyState — rendered by `CatalogScreen` when a user has zero sources
 * and zero photos. Replaces the old 4-step onboarding wizard.
 *
 * No hero copy, no welcome paragraph — just the mode chooser + the three
 * "Add source" actions (`AddSourcePopover`). The user explicitly picks a
 * mode before the action buttons unlock, which forces the "what will this
 * do to my files?" decision up front instead of burying it in settings.
 */

import { AddSourcePopover } from './AddSourcePopover';

export function CatalogEmptyState() {
  return (
    <div
      style={{
        width: '100%',
        display: 'flex',
        justifyContent: 'center',
        padding: '60px 24px',
      }}
    >
      <AddSourcePopover layout="block" />
    </div>
  );
}
