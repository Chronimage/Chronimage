import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import { Icon, type IconName } from '../primitives/Icon';
import { SCREENS, type Screen, type ScreenId } from '../state/ui';

export interface RailProps {
  readonly screen: Screen;
  readonly onScreenChange: (id: ScreenId) => void;
}

interface RailItem {
  readonly id: ScreenId;
  readonly icon: IconName;
  readonly label: string;
  readonly hint?: string;
}

const PRIMARY: readonly RailItem[] = [
  { id: 'catalog', icon: 'grid', label: 'Catalog', hint: 'Browse · 1' },
  { id: 'cull', icon: 'cull', label: 'Cull', hint: 'Compare · 2' },
  { id: 'develop', icon: 'brush', label: 'Develop', hint: 'Edit · 3' },
];

const SECONDARY: readonly RailItem[] = [{ id: 'settings', icon: 'settings', label: SCREENS.settings.label }];

export function Rail({ screen, onScreenChange }: RailProps) {
  return (
    <TooltipProvider delayDuration={250}>
      <nav aria-label="Primary" className="rail">
        <div className="rail-stack">
          {PRIMARY.map((it) => (
            <RailButton
              key={it.id}
              item={it}
              active={screen.id === it.id}
              onSelect={() => onScreenChange(it.id)}
            />
          ))}
        </div>
        <div className="rail-stack">
          {SECONDARY.map((it) => (
            <RailButton
              key={it.id}
              item={it}
              active={screen.id === it.id}
              onSelect={() => onScreenChange(it.id)}
            />
          ))}
        </div>
      </nav>
    </TooltipProvider>
  );
}

function RailButton({
  item,
  active,
  onSelect,
}: {
  readonly item: RailItem;
  readonly active: boolean;
  readonly onSelect: () => void;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          onClick={onSelect}
          aria-label={item.label}
          aria-current={active ? 'page' : undefined}
          className={cn('rail-btn', active && 'rail-btn-active')}
        >
          <Icon name={item.icon} size={16} />
        </button>
      </TooltipTrigger>
      <TooltipContent side="right" sideOffset={8} className="font-mono uppercase tracking-[0.08em]">
        <span className="text-[var(--text-2xs)]">{item.label}</span>
        {item.hint && (
          <span className="ml-2 text-[var(--text-2xs)] text-[color:var(--fg-mute)]">{item.hint}</span>
        )}
      </TooltipContent>
    </Tooltip>
  );
}
