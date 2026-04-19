import type { CSSProperties, ReactNode } from 'react';

export type ChipTone = 'info' | 'warn' | 'danger';
export type ChipVariant = 'solid';

export interface ChipProps {
  children: ReactNode;
  tone?: ChipTone;
  variant?: ChipVariant;
  onClose?: () => void;
  onClick?: () => void;
  style?: CSSProperties;
  className?: string;
}

export function Chip({ children, tone, variant, onClose, onClick, style, className }: ChipProps) {
  const toneClass = tone ?? '';
  const variantClass = variant ?? '';
  const classes = ['chip', toneClass, variantClass, className].filter(Boolean).join(' ');
  const showDot = tone !== undefined || variant === 'solid';

  const content = (
    <>
      {showDot && <span className="dot" />}
      {children}
      {onClose && (
        <button
          type="button"
          className="x"
          onClick={(e) => {
            e.stopPropagation();
            onClose();
          }}
          aria-label="Remove"
        >
          ×
        </button>
      )}
    </>
  );

  if (onClick) {
    return (
      <button type="button" className={classes} onClick={onClick} style={{ cursor: 'pointer', ...style }}>
        {content}
      </button>
    );
  }

  return (
    <span className={classes} style={style}>
      {content}
    </span>
  );
}
