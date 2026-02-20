import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { FileAttachment } from "../../../components/chat/FileAttachment";
import { useAppStore } from "../../../store";
import type { FileAttachment as FileAttachmentType } from "../../../types";

function renderAttachment(attachment: FileAttachmentType) {
  useAppStore.setState(useAppStore.getInitialState(), true);
  return render(<FileAttachment attachment={attachment} />);
}

describe("FileAttachment", () => {
  it("renders image thumbnail for image MIME types", () => {
    renderAttachment({
      id: "att-1",
      fileName: "photo.png",
      fileSize: 204800,
      mimeType: "image/png",
      url: "https://example.com/photo.png",
      thumbnailUrl: "https://example.com/photo-thumb.png",
    });

    const img = screen.getByAltText("photo.png");
    expect(img).toBeInTheDocument();
    expect(img).toHaveAttribute("src", "https://example.com/photo-thumb.png");
  });

  it("renders file card for non-image attachments", () => {
    renderAttachment({
      id: "att-2",
      fileName: "report.pdf",
      fileSize: 2621440,
      mimeType: "application/pdf",
      url: "https://example.com/report.pdf",
      thumbnailUrl: null,
    });

    expect(screen.getByText("report.pdf")).toBeInTheDocument();
    expect(screen.getByText("2.5 MB")).toBeInTheDocument();
  });

  it("renders a download button", () => {
    renderAttachment({
      id: "att-3",
      fileName: "data.csv",
      fileSize: 1024,
      mimeType: "text/csv",
      url: "https://example.com/data.csv",
      thumbnailUrl: null,
    });

    expect(screen.getByLabelText("Download data.csv")).toBeInTheDocument();
  });
});
