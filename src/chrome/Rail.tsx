import { Icon, type IconName } from '../primitives/Icon';
import { SCREENS, type Screen, type ScreenId } from '../state/ui';

export interface RailProps {
  screen: Screen;
  onScreenChange: (id: ScreenId) => void;
}

interface RailItem {
  id: ScreenId;
  icon: IconName;
  label: string;
}

const ITEMS: RailItem[] = [
  { id: 'catalog', icon: 'grid', label: 'Catalog' },
  { id: 'people', icon: 'faces', label: 'People' },
  { id: 'cull', icon: 'cull', label: 'Cull' },
  { id: 'cullbin', icon: 'flag', label: 'Cull Bin' },
  { id: 'develop', icon: 'brush', label: 'Develop' },
];

export function Rail({ screen, onScreenChange }: RailProps) {
  return (
    <div className="rail">
      {ITEMS.map((it) => (
        <button
          type="button"
          key={it.id}
          className={screen.id === it.id ? 'active' : ''}
          onClick={() => onScreenChange(it.id)}
          title={it.label}
          aria-label={it.label}
        >
          <Icon name={it.icon} size={16} />
        </button>
      ))}
      <div style={{ flex: 1 }} />
      <button
        type="button"
        className={screen.id === 'settings' ? 'active' : ''}
        onClick={() => onScreenChange('settings')}
        title={SCREENS.settings.label}
        aria-label="Settings"
      >
        <Icon name="settings" size={16} />
      </button>
    </div>
  );
}
