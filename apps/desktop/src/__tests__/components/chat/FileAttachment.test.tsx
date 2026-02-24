import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { FileAttachment } from "../../../components/chat/FileAttachment";
import { useAppStore } from "../../../store";
import type { FileAttachment as FileAttachmentType } from "../../../types";

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: vi.fn((path: string) => `asset://localhost/${path}`),
}));

function renderAttachment(overrides: Partial<FileAttachmentType> = {}) {
  useAppStore.setState(useAppStore.getInitialState(), true);
  const attachment: FileAttachmentType = {
    id: "att-1",
    fileName: "photo.png",
    fileSize: 204800,
    mimeType: "image/png",
    url: "https://example.com/photo.png",
    thumbnailUrl: "https://example.com/photo-thumb.png",
    localPath: null,
    thumbnailLocalPath: null,
    ...overrides,
  };
  return render(<FileAttachment attachment={attachment} />);
}

describe("FileAttachment", () => {
  it("renders image thumbnail for image MIME types", () => {
    renderAttachment();

    const img = screen.getByAltText("photo.png");
    expect(img).toBeInTheDocument();
    expect(img).toHaveAttribute("src", "https://example.com/photo-thumb.png");
  });

  it("uses local thumbnail path when available", () => {
    renderAttachment({
      thumbnailLocalPath: "/app/thumbnails/att-1_thumb.jpg",
    });

    const img = screen.getByAltText("photo.png");
    expect(img).toHaveAttribute(
      "src",
      "asset://localhost//app/thumbnails/att-1_thumb.jpg",
    );
  });

  it("uses local path for full image when available", () => {
    renderAttachment({
      localPath: "/app/attachments/att-1.png",
      thumbnailLocalPath: "/app/thumbnails/att-1_thumb.jpg",
    });

    const img = screen.getByAltText("photo.png");
    // Thumbnail src uses thumbnailLocalPath
    expect(img).toHaveAttribute(
      "src",
      "asset://localhost//app/thumbnails/att-1_thumb.jpg",
    );
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

  it("uses local path for download link when available", () => {
    renderAttachment({
      fileName: "data.csv",
      fileSize: 1024,
      mimeType: "text/csv",
      url: "https://example.com/data.csv",
      thumbnailUrl: null,
      localPath: "/app/attachments/data.csv",
    });

    const link = screen.getByLabelText("Download data.csv");
    expect(link).toHaveAttribute(
      "href",
      "asset://localhost//app/attachments/data.csv",
    );
  });
});
