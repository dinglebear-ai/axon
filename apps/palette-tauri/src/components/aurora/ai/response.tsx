import * as React from "react";

import { MarkdownBody } from "@/components/palette/MarkdownBody";

export interface ResponseSource {
  title: string;
  href?: string;
  description?: string;
}

export interface ResponseProps extends React.HTMLAttributes<HTMLDivElement> {
  markdown: string;
  sources?: ResponseSource[];
  streaming?: boolean;
}

const Response = React.forwardRef<HTMLDivElement, ResponseProps>(
  ({ className, markdown, streaming = false, ...props }, ref) => (
    <div
      ref={ref}
      className={["aurora-response", streaming ? "aurora-response-streaming" : "", className].filter(Boolean).join(" ")}
      aria-busy={streaming || undefined}
      aria-live={streaming ? "polite" : undefined}
      {...props}
    >
      <MarkdownBody>{markdown}</MarkdownBody>
      {streaming ? <span className="aurora-response-caret" aria-hidden="true" /> : null}
    </div>
  ),
);
Response.displayName = "Response";

export { Response };
export default Response;
