import { describe, it, expect } from "vitest";
import { screen, fireEvent } from "@testing-library/react";
import { renderWithProviders } from "../../helpers/renderWithProviders";
import { ImageViewer } from "../../../components/modals/ImageViewer";
import { useAppStore } from "../../../store";

const testImageUrl = "https://example.com/test-image.png";

function renderViewer(props?: { allImages?: string[] }) {
  useAppStore.setState({
    activeModal: { type: "imageViewer", props: { imageUrl: testImageUrl } },
  });
  return renderWithProviders(
    <ImageViewer imageUrl={testImageUrl} allImages={props?.allImages} />,
  );
}

describe("ImageViewer", () => {
  it("renders full-screen image overlay", () => {
    renderViewer();

    const img = screen.getByRole("img");
    expect(img).toBeInTheDocument();
    expect(img).toHaveAttribute("src", testImageUrl);
    expect(screen.getByTestId("image-viewer-backdrop")).toBeInTheDocument();
  });

  it("Escape key closes the viewer", () => {
    renderViewer();

    useAppStore.setState({
      activeModal: { type: "imageViewer", props: { imageUrl: testImageUrl } },
    });

    fireEvent.keyDown(document, { key: "Escape" });

    expect(useAppStore.getState().activeModal).toBeNull();
  });

  it("clicking outside the image closes the viewer", () => {
    renderViewer();

    useAppStore.setState({
      activeModal: { type: "imageViewer", props: { imageUrl: testImageUrl } },
    });

    fireEvent.click(screen.getByTestId("image-viewer-backdrop"));

    expect(useAppStore.getState().activeModal).toBeNull();
  });

  it("clicking on the image does not close the viewer", () => {
    renderViewer();

    useAppStore.setState({
      activeModal: { type: "imageViewer", props: { imageUrl: testImageUrl } },
    });

    fireEvent.click(screen.getByRole("img"));

    expect(useAppStore.getState().activeModal).not.toBeNull();
  });
});
