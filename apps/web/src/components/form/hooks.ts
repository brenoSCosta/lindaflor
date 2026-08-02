import { createFormHook } from "@tanstack/react-form";

import {
  FormButton,
  FormCheckbox,
  FormError,
  FormField,
  FormFileUpload,
  FormInput,
  FormInputGroupInput,
  FormInputOTP,
  FormLabel,
  FormNumberInput,
  FormPasswordInput,
  FormRadioGroup,
  FormRadioGroupItem,
  FormSelect,
  FormSelectContent,
  FormSelectItem,
  FormSelectTrigger,
  FormSwitch,
  FormTextarea,
} from "@/components/form";
import { fieldContext, formContext } from "@/components/form/context";

export const { useAppForm, withForm } = createFormHook({
  fieldComponents: {
    Input: FormInput,
    FileUpload: FormFileUpload,
    NumberInput: FormNumberInput,
    Label: FormLabel,
    Field: FormField,
    Error: FormError,
    Textarea: FormTextarea,
    Checkbox: FormCheckbox,
    InputOTP: FormInputOTP,
    InputGroupInput: FormInputGroupInput,
    PasswordInput: FormPasswordInput,
    RadioGroup: FormRadioGroup,
    RadioGroupItem: FormRadioGroupItem,
    Select: FormSelect,
    SelectTrigger: FormSelectTrigger,
    SelectContent: FormSelectContent,
    SelectItem: FormSelectItem,
    Switch: FormSwitch,
  },
  formComponents: {
    Button: FormButton,
  },
  fieldContext,
  formContext,
});
