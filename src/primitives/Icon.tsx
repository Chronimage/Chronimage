/**
 * Icon — single line-icon set powered by lucide-react.
 *
 * The `name` union is preserved from the legacy custom-SVG version so
 * existing call sites (`<Icon name="grid" />`) keep working without a
 * sweeping refactor. New code can import the underlying lucide icons
 * directly when more icons are needed.
 *
 * All icons render as decorative (aria-hidden); parents must provide
 * accessible labels on the surrounding button/link.
 */

import {
  Brush,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Cloud,
  Columns2,
  CreditCard,
  Crop,
  Database,
  Download,
  Eye,
  Flag,
  HardDrive,
  History,
  Home,
  Layers,
  LayoutGrid,
  Link2,
  type LucideIcon,
  MapPin,
  MessageSquare,
  Minus,
  PanelsTopLeft,
  Plus,
  Search,
  Server,
  Settings,
  Smartphone,
  Sparkles,
  Square,
  Star,
  Tag,
  Trash2,
  Upload,
  Usb,
  User,
  Wand2,
  X,
} from 'lucide-react';

export type IconName =
  | 'home'
  | 'grid'
  | 'layers'
  | 'cull'
  | 'wand'
  | 'sparkles'
  | 'prompt'
  | 'search'
  | 'export'
  | 'settings'
  | 'link'
  | 'disk'
  | 'cloud'
  | 'nas'
  | 'card'
  | 'iphone'
  | 'android'
  | 'usb'
  | 'close'
  | 'min'
  | 'max'
  | 'chevR'
  | 'chevL'
  | 'chevD'
  | 'plus'
  | 'keep'
  | 'reject'
  | 'star'
  | 'flag'
  | 'tag'
  | 'eye'
  | 'history'
  | 'download'
  | 'crop'
  | 'brush'
  | 'faces'
  | 'ai'
  | 'compare'
  | 'pin'
  | 'cards';

const REGISTRY: Record<IconName, LucideIcon> = {
  home: Home,
  grid: LayoutGrid,
  layers: Layers,
  cull: Trash2,
  wand: Wand2,
  sparkles: Sparkles,
  prompt: MessageSquare,
  search: Search,
  export: Upload,
  settings: Settings,
  link: Link2,
  disk: HardDrive,
  cloud: Cloud,
  nas: Server,
  card: Database,
  iphone: Smartphone,
  android: Smartphone,
  usb: Usb,
  close: X,
  min: Minus,
  max: Square,
  chevR: ChevronRight,
  chevL: ChevronLeft,
  chevD: ChevronDown,
  plus: Plus,
  keep: Check,
  reject: X,
  star: Star,
  flag: Flag,
  tag: Tag,
  eye: Eye,
  history: History,
  download: Download,
  crop: Crop,
  brush: Brush,
  faces: User,
  ai: Sparkles,
  compare: Columns2,
  pin: MapPin,
  cards: PanelsTopLeft,
};

export interface IconProps {
  name: IconName;
  size?: number;
  stroke?: number;
  className?: string;
}

export function Icon({ name, size = 16, stroke = 1.75, className }: IconProps) {
  const Lucide = REGISTRY[name];
  return (
    <Lucide
      width={size}
      height={size}
      strokeWidth={stroke}
      aria-hidden="true"
      focusable={false}
      className={className}
    />
  );
}

/**
 * Re-export of `CreditCard` so screens that need a memory-card icon can
 * import it directly. Kept here because the legacy `card` mapping points
 * at `Database` (sd-card-shaped) which is a better fit for the source-rail
 * iconography; this is for explicit credit-card use cases.
 */
export { CreditCard };
