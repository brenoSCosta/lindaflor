import { MAX_FILE_SIZE_BYTES } from "@lindaflor/shared/constants";
import { Eye, EyeOff, Loader2, UploadIcon, XIcon } from "lucide-react";
import React from "react";
import { useTimeout } from "usehooks-ts";

import { useFieldContext, useFormContext } from "@/components/form/context";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import {
  FileUpload,
  FileUploadClearTrigger,
  FileUploadDescription,
  FileUploadDropzone,
  FileUploadDropzoneIcon,
  FileUploadItem,
  FileUploadItemDeleteTrigger,
  FileUploadItemGroup,
  FileUploadItemName,
  FileUploadItemPreview,
  FileUploadItemSize,
  FileUploadTitle,
  useFileUpload,
} from "@/components/ui/file-upload";
import { Input } from "@/components/ui/input";
import {
  InputGroup,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group";
import { InputOTP } from "@/components/ui/input-otp";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";

export function FormInput({ ...props }: React.ComponentProps<typeof Input>) {
  const field = useFieldContext<string>();
  const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid;

  return (
    <Input
      aria-invalid={isInvalid}
      id={field.name}
      name={field.name}
      onBlur={field.handleBlur}
      onChange={(e) => field.handleChange(e.target.value)}
      value={field.state.value}
      {...props}
    />
  );
}

export function FormNumberInput({
  ...props
}: React.ComponentProps<typeof Input>) {
  const field = useFieldContext<number>();
  const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid;
  const [raw, setRaw] = React.useState(() => String(field.state.value ?? ""));

  return (
    <Input
      id={field.name}
      name={field.name}
      onBlur={() => {
        setRaw(String(field.state.value ?? ""));
        field.handleBlur();
      }}
      {...props}
      type="text"
      aria-invalid={isInvalid}
      inputMode="decimal"
      onChange={(e) => {
        const next = e.target.value;
        if (next === "" || next === "-" || next === "." || next === "-.") {
          setRaw(next);
          return;
        }
        const num = Number(next);
        if (!Number.isNaN(num)) {
          setRaw(next);
          field.handleChange(num);
        }
      }}
      value={raw}
    />
  );
}

export function FormLabel({
  ...props
}: React.ComponentProps<typeof FieldLabel>) {
  const field = useFieldContext();
  return <FieldLabel htmlFor={field.name} {...props} />;
}

export function FormField({ ...props }: React.ComponentProps<typeof Field>) {
  const field = useFieldContext();
  const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid;

  return (
    <Field
      aria-invalid={isInvalid}
      data-invalid={isInvalid ? "true" : undefined}
      {...props}
    />
  );
}

export function FormError({
  ...props
}: React.ComponentProps<typeof FieldError>) {
  const field = useFieldContext();
  const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid;

  return isInvalid ? (
    <FieldError errors={field.state.meta.errors} {...props} />
  ) : null;
}

export function FormButton({
  type = "submit",
  spinner,
  spinnerPosition = "left",
  loading = false,
  loadingText = "Loading...",
  ...props
}: React.ComponentProps<typeof Button> & {
  spinner?: React.ReactNode;
  loadingText?: React.ReactNode;
  spinnerPosition?: "left" | "right";
  loading?: boolean;
}) {
  const form = useFormContext();

  const spinnerprop = spinner ?? (
    <Loader2 className="size-4 animate-spin" data-slot="spinner" />
  );

  const loadingTextElement =
    typeof loadingText === "string" ? (
      <span className="hidden md:block" data-slot="loading-text">
        {loadingText}
      </span>
    ) : (
      loadingText
    );

  return (
    <form.Subscribe>
      {({ isSubmitting, canSubmit }) => (
        <Button
          aria-disabled={!canSubmit}
          disabled={isSubmitting || props.disabled}
          type={type}
          {...props}
        >
          {isSubmitting || loading ? (
            <>
              {spinnerPosition === "left" && spinnerprop}
              {loadingTextElement}
              {spinnerPosition === "right" && spinnerprop}
            </>
          ) : (
            props.children
          )}
        </Button>
      )}
    </form.Subscribe>
  );
}

export function FormTextarea({
  ...props
}: React.ComponentProps<typeof Textarea>) {
  const field = useFieldContext<string>();
  const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid;
  return (
    <Textarea
      aria-invalid={isInvalid}
      id={field.name}
      name={field.name}
      onBlur={field.handleBlur}
      onChange={(e) => field.handleChange(e.target.value)}
      value={field.state.value}
      {...props}
    />
  );
}

export function FormCheckbox({
  ...props
}: React.ComponentProps<typeof Checkbox>) {
  const field = useFieldContext<boolean>();
  const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid;

  return (
    <Checkbox
      aria-invalid={isInvalid}
      checked={field.state.value}
      id={field.name}
      name={field.name}
      onBlur={field.handleBlur}
      onCheckedChange={(e) => field.handleChange(e)}
      {...props}
    />
  );
}

export function FormInputOTP({
  ...props
}: React.ComponentProps<typeof InputOTP>) {
  const field = useFieldContext<string>();
  const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid;

  return (
    <InputOTP
      aria-invalid={isInvalid}
      id={field.name}
      name={field.name}
      onBlur={field.handleBlur}
      onChange={field.handleChange}
      value={field.state.value}
      {...props}
    />
  );
}

const AUTO_HIDE_DELAY = 10_000;

export function FormPasswordInput({
  ...props
}: Omit<React.ComponentProps<typeof InputGroupInput>, "type">) {
  const field = useFieldContext<string>();
  const [showPassword, setShowPassword] = React.useState(false);
  const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid;

  useTimeout(
    () => {
      setShowPassword(false);
    },
    showPassword ? AUTO_HIDE_DELAY : null,
  );

  return (
    <InputGroup>
      <InputGroupInput
        aria-invalid={isInvalid}
        id={field.name}
        name={field.name}
        onBlur={field.handleBlur}
        onChange={(e) => field.handleChange(e.target.value)}
        type={showPassword ? "text" : "password"}
        value={field.state.value}
        {...props}
      />
      <InputGroupButton
        size="icon-sm"
        type="button"
        aria-label={showPassword ? "Hide password" : "Show password"}
        aria-pressed={showPassword}
        onClick={() => setShowPassword(!showPassword)}
      >
        {showPassword ? (
          <EyeOff className="size-4" />
        ) : (
          <Eye className="size-4" />
        )}
      </InputGroupButton>
    </InputGroup>
  );
}

export function FormInputGroupInput({
  ...props
}: React.ComponentProps<typeof InputGroupInput>) {
  const field = useFieldContext<string>();
  const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid;

  return (
    <InputGroupInput
      aria-invalid={isInvalid}
      id={field.name}
      name={field.name}
      onBlur={field.handleBlur}
      onChange={(e) => field.handleChange(e.target.value)}
      value={field.state.value}
      {...props}
    />
  );
}

export function FormSelect({
  children,
  ...props
}: React.ComponentProps<typeof Select>) {
  const field = useFieldContext();

  return (
    <Select
      value={field.state.value}
      onValueChange={(value) => {
        field.handleChange(value);
        field.handleBlur();
      }}
      {...props}
    >
      {children}
    </Select>
  );
}

export function FormSelectTrigger({
  children,
  ...props
}: React.ComponentProps<typeof SelectTrigger>) {
  const field = useFieldContext<string>();
  const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid;

  return (
    <SelectTrigger aria-invalid={isInvalid} id={field.name} {...props}>
      {children ?? <SelectValue />}
    </SelectTrigger>
  );
}

export function FormSelectContent({
  ...props
}: React.ComponentProps<typeof SelectContent>) {
  return <SelectContent {...props} />;
}

export function FormSelectItem({
  ...props
}: React.ComponentProps<typeof SelectItem>) {
  return <SelectItem {...props} />;
}

interface FormFileUploadProps {
  accept?: string;
  maxSizeBytes?: number;
  placeholder?: string;
  description?: string;
  className?: string;
}

function humanReadableSize(bytes: number): string {
  if (bytes === 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB"];
  const exponent = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1,
  );
  return `${(bytes / 1024 ** exponent).toFixed(exponent > 0 ? 1 : 0)} ${units[exponent]}`;
}

function FileUploadSelectedItem({ maxSizeBytes }: { maxSizeBytes: number }) {
  const fileUpload = useFileUpload();
  const field = useFieldContext<File | null>();
  const file = fileUpload.acceptedFiles[0];

  if (!file) {
    return null;
  }

  const isOversized = file.size > maxSizeBytes;

  return (
    <FileUploadItemGroup className="flex flex-col gap-2">
      <FileUploadItem
        className={cn(
          "flex w-full items-center gap-3 rounded-xl border bg-card p-3 animate-in fade-in-0 slide-in-from-top-5 motion-reduce:animate-none!",
          isOversized && "border-destructive bg-destructive/5",
        )}
        file={file}
      >
        <FileUploadItemPreview className="size-10" type=".*">
          <span className="uppercase">{file.name.split(".").pop()}</span>
        </FileUploadItemPreview>
        <div className="min-w-0 flex-1 overflow-hidden">
          <FileUploadItemName />
          <FileUploadItemSize />
          {isOversized && (
            <p className="text-xs text-destructive">
              O arquivo excede o limite de {humanReadableSize(maxSizeBytes)}.
            </p>
          )}
        </div>
        <FileUploadItemDeleteTrigger asChild>
          <button
            type="button"
            onClick={() => {
              field.handleChange(null);
            }}
            className="inline-flex size-7 items-center justify-center rounded-lg text-muted-foreground hover:bg-muted hover:text-foreground"
            aria-label="Remover arquivo"
          >
            <XIcon className="size-4" />
          </button>
        </FileUploadItemDeleteTrigger>
      </FileUploadItem>
    </FileUploadItemGroup>
  );
}

export function FormFileUpload({
  accept,
  maxSizeBytes = MAX_FILE_SIZE_BYTES,
  placeholder = "Arraste um arquivo ou clique para selecionar",
  description,
  className,
  ...props
}: FormFileUploadProps) {
  const field = useFieldContext<File | null>();
  const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid;

  return (
    <FileUpload
      className={className}
      accept={accept ? [accept] : undefined}
      maxFiles={1}
      onFileAccept={(details) => {
        const file = details.files[0];
        if (file) {
          field.handleChange(file);
          field.handleBlur();
        }
      }}
      onFileReject={() => {
        field.handleChange(null);
        field.handleBlur();
      }}
      {...props}
    >
      <FileUploadClearTrigger asChild>
        <div className="contents">
          <FileUploadDropzone
            className={cn(
              "min-h-32",
              isInvalid &&
                "border-destructive bg-destructive/5 dark:border-destructive-foreground",
            )}
          >
            <FileUploadDropzoneIcon>
              <UploadIcon />
            </FileUploadDropzoneIcon>
            <div className="space-y-1">
              <FileUploadTitle>{placeholder}</FileUploadTitle>
              {description && (
                <FileUploadDescription>{description}</FileUploadDescription>
              )}
              <FileUploadDescription>
                Máximo {humanReadableSize(maxSizeBytes)}
              </FileUploadDescription>
            </div>
          </FileUploadDropzone>
        </div>
      </FileUploadClearTrigger>

      <FileUploadSelectedItem maxSizeBytes={maxSizeBytes} />
    </FileUpload>
  );
}

export function FormSwitch({ ...props }: React.ComponentProps<typeof Switch>) {
  const field = useFieldContext<boolean>();
  const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid;

  return (
    <Switch
      checked={field.state.value}
      onCheckedChange={(checked) => field.handleChange(checked)}
      onBlur={field.handleBlur}
      aria-invalid={isInvalid}
      {...props}
    />
  );
}

export function FormRadioGroup({
  ...props
}: React.ComponentProps<typeof RadioGroup>) {
  const field = useFieldContext<string>();
  const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid;

  return (
    <RadioGroup
      aria-invalid={isInvalid}
      name={field.name}
      onBlur={field.handleBlur}
      onValueChange={(value) => {
        field.handleChange(value);
        field.handleBlur();
      }}
      value={field.state.value}
      {...props}
    />
  );
}

export function FormRadioGroupItem({
  ...props
}: React.ComponentProps<typeof RadioGroupItem>) {
  return <RadioGroupItem {...props} />;
}
