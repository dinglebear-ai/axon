import * as React from "react";

export interface MessageProps extends React.HTMLAttributes<HTMLElement> {
  role?: "assistant" | "user" | "system";
  time?: React.ReactNode;
  actions?: React.ReactNode;
}

export interface MessageContentProps extends React.HTMLAttributes<HTMLDivElement> {
  tone?: MessageProps["role"];
  streaming?: boolean;
}

const Message = React.forwardRef<HTMLElement, MessageProps>(
  ({ className, role = "assistant", time, actions, style, children, ...props }, ref) => {
    const isUser = role === "user";
    const hasMeta = time != null || actions != null;
    return (
      <article
        ref={ref}
        className={["aurora-message group/aurora-msg", className].filter(Boolean).join(" ")}
        data-role={role}
        style={{ alignItems: isUser ? "flex-end" : "stretch", ...style }}
        {...props}
      >
        <div className="aurora-message-row" style={{ justifyContent: isUser ? "flex-end" : "flex-start" }}>
          {children}
        </div>
        {hasMeta ? (
          <div className="aurora-message-meta" style={{ justifyContent: isUser ? "flex-end" : "flex-start" }}>
            {time != null ? <span className="aurora-message-time">{time}</span> : null}
            {actions}
          </div>
        ) : null}
      </article>
    );
  },
);
Message.displayName = "Message";

const MessageActionButton = React.forwardRef<HTMLButtonElement, React.ButtonHTMLAttributes<HTMLButtonElement>>(
  ({ className, type = "button", ...props }, ref) => (
    <button ref={ref} type={type} className={["aurora-message-action", className].filter(Boolean).join(" ")} {...props} />
  ),
);
MessageActionButton.displayName = "MessageActionButton";

const MessageContent = React.forwardRef<HTMLDivElement, MessageContentProps>(
  ({ className, tone = "assistant", streaming = false, children, ...props }, ref) => (
    <div
      ref={ref}
      className={[
        "aurora-message-content",
        `aurora-message-content-${tone}`,
        streaming ? "aurora-message-content-streaming" : "",
        className,
      ].filter(Boolean).join(" ")}
      {...props}
    >
      {children}
      {streaming ? <span className="aurora-message-caret" aria-hidden="true" /> : null}
    </div>
  ),
);
MessageContent.displayName = "MessageContent";

export { Message, MessageActionButton, MessageContent };
