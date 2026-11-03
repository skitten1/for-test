import { Component, Input } from '@angular/core';

@Component({
  selector: 'ngx-roles-system-badge-cell',
  template: `
    <nb-badge
      [text]="value ? 'Yes' : 'No'"
      [status]="value ? 'warning' : 'basic'"
      size="small"
    ></nb-badge>
  `,
})
export class RolesSystemBadgeCellComponent {
  @Input() value = false;
}


