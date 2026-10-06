import { useEffect, useRef } from "react";

type Props = { uri: string; onMessage: (data: unknown) => void };

export default function MapPickerFrame({ uri, onMessage }: Props) {
  const frame = useRef<HTMLIFrameElement>(null);
  useEffect(() => {
    const origin = new URL(uri).origin;
    const receive = (event: MessageEvent) => {
      if (event.source === frame.current?.contentWindow && event.origin === origin) onMessage(event.data);
    };
    window.addEventListener("message", receive);
    return () => window.removeEventListener("message", receive);
  }, [uri, onMessage]);
  return <iframe ref={frame} src={uri} title="选择活动地点" allow="geolocation"
    style={{ flex: 1, width: "100%", border: 0, background: "#fff" }} />;
}
