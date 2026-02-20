import { useState, useRef } from "react";
import { Modal } from "../ui/Modal";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { useAppStore } from "../../store";

export function CreateGuildModal() {
  const [name, setName] = useState("");
  const [iconPreview, setIconPreview] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const createGuild = useAppStore((s) => s.createGuild);
  const closeModal = useAppStore((s) => s.closeModal);

  const handleIconSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      setIconPreview(reader.result as string);
    };
    reader.readAsDataURL(file);
  };

  const handleCreate = () => {
    if (!name.trim()) return;
    createGuild(name.trim(), iconPreview);
    closeModal();
  };

  return (
    <Modal open onClose={closeModal} title="Create a Server">
      <div className="flex flex-col gap-5">
        <div className="flex flex-col items-center gap-2">
          <button
            type="button"
            onClick={() => fileInputRef.current?.click()}
            aria-label="Upload server icon"
            className="w-20 h-20 rounded-2xl bg-[var(--bg-tertiary)] border-2 border-dashed border-[var(--border-primary)] flex items-center justify-center overflow-hidden hover:border-[var(--bg-accent)] transition-all duration-200 cursor-pointer"
          >
            {iconPreview ? (
              <img
                src={iconPreview}
                alt="Server icon preview"
                className="w-full h-full object-cover"
              />
            ) : (
              <span className="text-[var(--text-muted)] text-xs text-center leading-tight">
                Upload<br />Icon
              </span>
            )}
          </button>
          <input
            ref={fileInputRef}
            type="file"
            accept="image/*"
            className="hidden"
            onChange={handleIconSelect}
            data-testid="icon-file-input"
          />
        </div>
        <Input
          label="Server Name"
          placeholder="Enter server name"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <div className="flex justify-end gap-2.5">
          <Button variant="ghost" onClick={closeModal}>
            Cancel
          </Button>
          <Button disabled={!name.trim()} onClick={handleCreate}>
            Create
          </Button>
        </div>
      </div>
    </Modal>
  );
}
