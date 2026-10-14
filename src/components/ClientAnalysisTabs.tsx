import { TabsList, TabsTrigger } from "@/components/ui/tabs";
import { CLIENT_LABELS, clientLabel, isClientKey } from "@/lib/client-labels";

const CLIENTS = Object.keys(CLIENT_LABELS).filter(isClientKey);

export function ClientAnalysisTabs() {
  return (
    <div className="min-w-0 border-b border-border">
      <div className="client-tabs-scroll min-w-0 overflow-x-auto overscroll-x-contain">
        <TabsList
          variant="line"
          aria-label="客户端选择"
          className="justify-start gap-6 px-3 py-0 group-data-[orientation=horizontal]/tabs:h-auto"
        >
          {[
            { key: "all", label: "全部客户端" },
            ...CLIENTS.map((client) => ({ key: client, label: clientLabel(client) })),
          ].map(({ key, label }) => (
            <TabsTrigger
              key={key}
              value={key}
              className="h-11 flex-none rounded-none border-0 px-0 py-3 text-sm after:rounded-full group-data-[orientation=horizontal]/tabs:after:inset-x-1 group-data-[orientation=horizontal]/tabs:after:bottom-0 focus-visible:ring-inset"
              onFocus={(event) =>
                event.currentTarget.scrollIntoView({ block: "nearest", inline: "nearest" })
              }
            >
              {label}
            </TabsTrigger>
          ))}
        </TabsList>
      </div>
    </div>
  );
}
