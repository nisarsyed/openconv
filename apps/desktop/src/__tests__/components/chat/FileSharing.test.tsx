import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { UploadPreview } from "../../../components/chat/UploadPreview";
import { FilePreviewCard } from "../../../components/chat/FilePreviewCard";
import { useAppStore } from "../../../store";

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: vi.fn((path: string) => `asset://localhost/${path}`),
}));

describe("UploadPreview", () => {
  it("shows file name and size", () => {
    render(
      <UploadPreview
        fileName="doc.pdf"
        fileSize={1048576}
        progress={0}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText("doc.pdf")).toBeInTheDocument();
    expect(screen.getByText("1.0 MB")).toBeInTheDocument();
  });

  it("shows upload progress percentage", () => {
    render(
      <UploadPreview
        fileName="doc.pdf"
        fileSize={1048576}
        progress={0.5}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText(/50%/)).toBeInTheDocument();
  });

  it("calls onCancel when cancel button clicked", async () => {
    const user = userEvent.setup();
    const onCancel = vi.fn();
    render(
      <UploadPreview
        fileName="doc.pdf"
        fileSize={1048576}
        progress={0.3}
        onCancel={onCancel}
      />,
    );

    await user.click(screen.getByLabelText("Cancel upload"));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("does not show progress when complete", () => {
    render(
      <UploadPreview
        fileName="doc.pdf"
        fileSize={1048576}
        progress={1}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.queryByText(/100%/)).not.toBeInTheDocument();
  });
});

describe("FilePreviewCard", () => {
  beforeEach(() => {
    useAppStore.setState(useAppStore.getInitialState(), true);
  });

  it("renders inline thumbnail for images", () => {
    render(
      <FilePreviewCard
        fileId="file-1"
        fileName="photo.png"
        fileSize={204800}
        mimeType="image/png"
        localPath="/app/attachments/file-1.png"
        thumbnailPath="/app/thumbnails/file-1_thumb.jpg"
        isDownloading={false}
        onDownload={vi.fn()}
      />,
    );

    const img = screen.getByAltText("photo.png");
    expect(img).toBeInTheDocument();
    expect(img).toHaveAttribute(
      "src",
      "asset://localhost//app/thumbnails/file-1_thumb.jpg",
    );
  });

  it("renders download button for non-image files", () => {
    render(
      <FilePreviewCard
        fileId="file-2"
        fileName="report.pdf"
        fileSize={2621440}
        mimeType="application/pdf"
        localPath="/app/attachments/file-2.pdf"
        thumbnailPath={null}
        isDownloading={false}
        onDownload={vi.fn()}
      />,
    );

    expect(screen.getByText("report.pdf")).toBeInTheDocument();
    expect(screen.getByText("2.5 MB")).toBeInTheDocument();
    expect(screen.getByLabelText("Save report.pdf")).toBeInTheDocument();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
  });

  it("shows downloading state when isDownloading is true", () => {
    render(
      <FilePreviewCard
        fileId="file-3"
        fileName="data.csv"
        fileSize={1024}
        mimeType="text/csv"
        localPath={null}
        thumbnailPath={null}
        isDownloading={true}
        onDownload={vi.fn()}
      />,
    );

    expect(screen.getByLabelText("Downloading...")).toBeInTheDocument();
  });

  it("calls onDownload when download button clicked", async () => {
    const user = userEvent.setup();
    const onDownload = vi.fn();
    render(
      <FilePreviewCard
        fileId="file-4"
        fileName="data.csv"
        fileSize={1024}
        mimeType="text/csv"
        localPath="/app/attachments/file-4.csv"
        thumbnailPath={null}
        isDownloading={false}
        onDownload={onDownload}
      />,
    );

    await user.click(screen.getByLabelText("Save data.csv"));
    expect(onDownload).toHaveBeenCalledOnce();
  });
});
