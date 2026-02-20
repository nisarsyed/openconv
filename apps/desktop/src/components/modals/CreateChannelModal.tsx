import { useState, useMemo } from "react";
import { Modal } from "../ui/Modal";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Button } from "../ui/Button";
import { useAppStore } from "../../store";

interface CreateChannelModalProps {
  guildId: string;
}

const NEW_CATEGORY_VALUE = "__new__";

export function CreateChannelModal({ guildId }: CreateChannelModalProps) {
  const [name, setName] = useState("");
  const [channelType, setChannelType] = useState<"text" | "voice">("text");
  const [category, setCategory] = useState<string>("");
  const [newCategoryName, setNewCategoryName] = useState("");

  const createChannel = useAppStore((s) => s.createChannel);
  const closeModal = useAppStore((s) => s.closeModal);
  const channelsById = useAppStore((s) => s.channelsById);
  const channelIds = useAppStore((s) => s.channelIdsByGuild[guildId] ?? []);

  const categories = useMemo(() => {
    const cats = new Set<string>();
    for (const id of channelIds) {
      const ch = channelsById[id];
      if (ch?.category) cats.add(ch.category);
    }
    return Array.from(cats);
  }, [channelIds, channelsById]);

  const handleNameChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const raw = e.target.value.toLowerCase().replace(/\s+/g, "-");
    setName(raw);
  };

  const handleCreate = () => {
    if (!name.trim()) return;
    const resolvedCategory =
      category === NEW_CATEGORY_VALUE
        ? newCategoryName.trim() || null
        : category || null;
    createChannel(guildId, name.trim(), channelType, resolvedCategory);
    closeModal();
  };

  const typeOptions = [
    { value: "text", label: "Text" },
    { value: "voice", label: "Voice" },
  ];

  const categoryOptions = [
    { value: "", label: "No Category" },
    ...categories.map((c) => ({ value: c, label: c })),
    { value: NEW_CATEGORY_VALUE, label: "New Category" },
  ];

  return (
    <Modal open onClose={closeModal} title="Create Channel">
      <div className="flex flex-col gap-4">
        <Input
          label="Channel Name"
          placeholder="new-channel"
          value={name}
          onChange={handleNameChange}
        />
        <Select
          label="Channel Type"
          options={typeOptions}
          value={channelType}
          onChange={(e) => setChannelType(e.target.value as "text" | "voice")}
        />
        <Select
          label="Category"
          options={categoryOptions}
          value={category}
          onChange={(e) => setCategory(e.target.value)}
        />
        {category === NEW_CATEGORY_VALUE && (
          <Input
            label="New Category Name"
            placeholder="Enter category name"
            value={newCategoryName}
            onChange={(e) => setNewCategoryName(e.target.value)}
          />
        )}
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={closeModal}>
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
