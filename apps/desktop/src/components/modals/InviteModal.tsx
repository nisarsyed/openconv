import { useState, useMemo, useEffect, useRef } from "react";
import { Modal } from "../ui/Modal";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Button } from "../ui/Button";
import { useAppStore } from "../../store";

interface InviteModalProps {
  guildId: string;
}

function generateInviteCode(): string {
  const chars =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  let code = "";
  for (let i = 0; i < 8; i++) {
    code += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return code;
}

export function InviteModal({ guildId: _guildId }: InviteModalProps) {
  const closeModal = useAppStore((s) => s.closeModal);
  const [expiration, setExpiration] = useState("7d");
  const [copied, setCopied] = useState(false);
  const copyTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  const inviteLink = useMemo(
    () => `https://openconv.app/invite/${generateInviteCode()}`,
    [],
  );

  useEffect(() => {
    return () => {
      if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
    };
  }, []);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(inviteLink);
    setCopied(true);
    if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
    copyTimerRef.current = setTimeout(() => setCopied(false), 2000);
  };

  const expirationOptions = [
    { value: "1h", label: "1 hour" },
    { value: "1d", label: "1 day" },
    { value: "7d", label: "7 days" },
    { value: "never", label: "Never" },
  ];

  return (
    <Modal open onClose={closeModal} title="Invite People">
      <div className="flex flex-col gap-4">
        <p className="text-sm text-[var(--text-secondary)]">
          Share this link with others to grant access to your server
        </p>
        <div className="flex gap-2">
          <Input
            readOnly
            value={inviteLink}
            aria-label="Invite link"
            className="flex-1"
          />
          <Button onClick={handleCopy}>{copied ? "Copied!" : "Copy"}</Button>
        </div>
        <Select
          label="Expiration"
          options={expirationOptions}
          value={expiration}
          onChange={(e) => setExpiration(e.target.value)}
        />
      </div>
    </Modal>
  );
}
