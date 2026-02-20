import { useAppStore } from "../../store";
import { CreateGuildModal } from "./CreateGuildModal";
import { CreateChannelModal } from "./CreateChannelModal";
import { InviteModal } from "./InviteModal";
import { ImageViewer } from "./ImageViewer";
import { ConfirmDialog } from "./ConfirmDialog";

export function ModalRoot() {
  const activeModal = useAppStore((s) => s.activeModal);

  if (!activeModal) return null;

  const props =
    (activeModal as Record<string, unknown>).props as
      | Record<string, unknown>
      | undefined;

  switch (activeModal.type) {
    case "createGuild":
      return <CreateGuildModal />;
    case "createChannel":
      if (!props?.guildId) return null;
      return <CreateChannelModal guildId={props.guildId as string} />;
    case "invite":
      if (!props?.guildId) return null;
      return <InviteModal guildId={props.guildId as string} />;
    case "imageViewer":
      if (!props?.imageUrl) return null;
      return (
        <ImageViewer
          imageUrl={props.imageUrl as string}
          allImages={props.allImages as string[] | undefined}
        />
      );
    case "confirm":
      if (!props?.title || !props?.onConfirm) return null;
      return (
        <ConfirmDialog
          title={props.title as string}
          message={(props.message as string) ?? ""}
          onConfirm={props.onConfirm as () => void}
        />
      );
    default:
      return null;
  }
}
