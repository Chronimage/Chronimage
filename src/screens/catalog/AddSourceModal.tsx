/**
 * AddSourceModal — full-screen dialog wrapping `AddSourcePopover`. Built on
 * shadcn `Dialog` with consistent typography classes (eyebrow / display-lg)
 * matching the rest of the app shell.
 */

import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { AddSourcePopover } from './AddSourcePopover';

export interface AddSourceModalProps {
  readonly open: boolean;
  readonly onClose: () => void;
}

export function AddSourceModal({ open, onClose }: AddSourceModalProps) {
  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent className="add-source-dialog">
        <DialogHeader className="add-source-dialog-head">
          <div className="eyebrow">Add a source</div>
          <DialogTitle asChild>
            <h2 className="display-lg">
              Bring in photos<em>.</em>
            </h2>
          </DialogTitle>
          <DialogDescription className="body-sm">
            Pick a folder, an iCloud sync drive, or connect Google Photos. Originals stay where they are until
            you choose to clean up.
          </DialogDescription>
        </DialogHeader>
        <div className="add-source-dialog-body">
          <AddSourcePopover layout="block" />
        </div>
      </DialogContent>
    </Dialog>
  );
}
