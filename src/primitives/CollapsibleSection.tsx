import type { ReactNode } from 'react';
import { useDevelopUi } from '../state/develop';
import { Icon } from './Icon';

export interface CollapsibleSectionProps {
  /** Stable identifier used to key panel-open state in the develop store. */
  id: string;
  /** Lightroom-style uppercase label rendered in the section head. */
  title: string;
  children: ReactNode;
  /** Optional right-aligned action (typically a reset button). */
  action?: ReactNode;
  /** Optional eye toggle. When undefined, the icon is not rendered. */
  visibility?: { visible: boolean; onToggle: () => void; label?: string };
  /** Default open state when no value has been recorded in the store. */
  defaultOpen?: boolean;
  /** When true the section never collapses (used for stages that must stay visible). */
  alwaysOpen?: boolean;
}

export function CollapsibleSection({
  id,
  title,
  children,
  action,
  visibility,
  defaultOpen = false,
  alwaysOpen = false,
}: CollapsibleSectionProps) {
  const open = useDevelopUi((s) => s.panelOpen[id] ?? defaultOpen);
  const setPanelOpen = useDevelopUi((s) => s.setPanelOpen);
  const isOpen = alwaysOpen || open;

  return (
    <section className="editor-section" data-open={isOpen}>
      <header className="editor-section-head">
        <button
          type="button"
          className="editor-section-disclosure"
          aria-expanded={isOpen}
          aria-controls={`section-${id}`}
          onClick={() => {
            if (!alwaysOpen) setPanelOpen(id, !isOpen);
          }}
          disabled={alwaysOpen}
        >
          <span className="editor-section-chevron" aria-hidden="true">
            <Icon name={isOpen ? 'chevD' : 'chevR'} size={11} stroke={1.8} />
          </span>
          <span className="editor-section-title">{title}</span>
        </button>
        <div className="editor-section-tools">
          {action}
          {visibility ? (
            <button
              type="button"
              className="editor-section-eye"
              onClick={visibility.onToggle}
              aria-pressed={visibility.visible}
              aria-label={visibility.label ?? `Toggle ${title} visibility`}
              title={visibility.visible ? 'Hide effect' : 'Show effect'}
              data-active={visibility.visible}
            >
              <Icon name="eye" size={12} />
            </button>
          ) : null}
        </div>
      </header>
      {isOpen ? (
        <div id={`section-${id}`} className="editor-section-body">
          {children}
        </div>
      ) : null}
    </section>
  );
}
