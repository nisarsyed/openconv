import { Modal } from "../ui/Modal";
import { Button } from "../ui/Button";
import { useAppStore } from "../../store";

interface ConfirmDialogProps {
  title: string;
  message: string;
  onConfirm: () => void;
  confirmLabel?: string;
}

export function ConfirmDialog({
  title,
  message,
  onConfirm,
  confirmLabel = "Confirm",
}: ConfirmDialogProps) {
  const closeModal = useAppStore((s) => s.closeModal);

  const handleConfirm = () => {
    onConfirm();
    closeModal();
  };

  return (
    <Modal open onClose={closeModal} title={title}>
      <div className="flex flex-col gap-4">
        <p className="text-sm text-[var(--text-secondary)]">{message}</p>
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={closeModal}>
            Cancel
          </Button>
          <Button variant="danger" onClick={handleConfirm}>
            {confirmLabel}
          </Button>
        </div>
      </div>
    </Modal>
  );
}
