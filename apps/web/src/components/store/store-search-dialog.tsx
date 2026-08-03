import { useNavigate } from "@tanstack/react-router";
import { Search } from "lucide-react";
import { useState, type ComponentProps } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";

type StoreSearchDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export function StoreSearchDialog({
  open,
  onOpenChange,
}: StoreSearchDialogProps) {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");

  const handleSubmit: NonNullable<ComponentProps<"form">["onSubmit"]> = (
    event,
  ) => {
    event.preventDefault();
    const trimmed = query.trim();
    if (!trimmed) {
      return;
    }

    onOpenChange(false);
    void navigate({
      to: "/produtos",
      search: { q: trimmed },
    });
    setQuery("");
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="border-(--lf-line) bg-(--lf-cream) sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="font-display text-2xl">
            Buscar produtos
          </DialogTitle>
        </DialogHeader>
        <form onSubmit={handleSubmit} className="flex gap-2">
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Ex: biquíni, maiô, saída..."
            className="rounded-none border-(--lf-line)"
            autoFocus
          />
          <Button
            type="submit"
            className="rounded-none bg-(--lf-pink) uppercase"
          >
            <Search className="size-4" />
          </Button>
        </form>
      </DialogContent>
    </Dialog>
  );
}
