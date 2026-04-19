using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class FormTaxPayInfo : Form
{
	private IContainer components;

	[field: AccessedThroughProperty("TaxL")]
	internal virtual ListBox TaxL
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("PayL")]
	internal virtual ListBox PayL
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label2")]
	internal virtual Label Label2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public FormTaxPayInfo()
	{
		((Form)this).Load += FormTaxPayInfo_Load;
		InitializeComponent();
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_0059: Unknown result type (might be due to invalid IL or missing references)
		//IL_0063: Expected O, but got Unknown
		//IL_00dd: Unknown result type (might be due to invalid IL or missing references)
		//IL_00e7: Expected O, but got Unknown
		//IL_0170: Unknown result type (might be due to invalid IL or missing references)
		//IL_017a: Expected O, but got Unknown
		//IL_01f4: Unknown result type (might be due to invalid IL or missing references)
		//IL_01fe: Expected O, but got Unknown
		//IL_02e0: Unknown result type (might be due to invalid IL or missing references)
		//IL_02ea: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormTaxPayInfo));
		TaxL = new ListBox();
		PayL = new ListBox();
		Label2 = new Label();
		Label1 = new Label();
		((Control)this).SuspendLayout();
		TaxL.Font = new Font("Consolas", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((ListControl)TaxL).FormattingEnabled = true;
		TaxL.ItemHeight = 23;
		((Control)TaxL).Location = new Point(12, 38);
		((Control)TaxL).Name = "TaxL";
		((Control)TaxL).Size = new Size(383, 372);
		((Control)TaxL).TabIndex = 0;
		PayL.Font = new Font("Consolas", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((ListControl)PayL).FormattingEnabled = true;
		PayL.ItemHeight = 23;
		((Control)PayL).Location = new Point(415, 38);
		((Control)PayL).Name = "PayL";
		((Control)PayL).Size = new Size(336, 372);
		((Control)PayL).TabIndex = 1;
		Label2.AutoSize = true;
		((Control)Label2).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label2).Location = new Point(12, 10);
		((Control)Label2).Name = "Label2";
		((Control)Label2).Size = new Size(243, 25);
		((Control)Label2).TabIndex = 2;
		Label2.Text = "Інформація про податки";
		Label1.AutoSize = true;
		((Control)Label1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label1).Location = new Point(410, 10);
		((Control)Label1).Name = "Label1";
		((Control)Label1).Size = new Size(243, 25);
		((Control)Label1).TabIndex = 3;
		Label1.Text = "Інформація про платежі";
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(765, 420);
		((Control)this).Controls.Add((Control)(object)Label1);
		((Control)this).Controls.Add((Control)(object)Label2);
		((Control)this).Controls.Add((Control)(object)PayL);
		((Control)this).Controls.Add((Control)(object)TaxL);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormTaxPayInfo";
		((Form)this).StartPosition = (FormStartPosition)4;
		((Form)this).Text = "Довідкова інформація";
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void FormTaxPayInfo_Load(object sender, EventArgs e)
	{
		LoadTax();
		LoadPay();
	}

	private void LoadPay()
	{
		int payN = All.PayTax.PayN;
		checked
		{
			for (int i = 0; i <= payN; i++)
			{
				if (i == 0)
				{
					string text = "  №      ім'я платежу";
					PayL.Items.Add((object)text);
					PayL.Items.Add((object)"-----------------------------");
				}
				else
				{
					string text2 = ((i.ToString().Length >= 2) ? (Strings.Space(1) + i) : (Strings.Space(1) + " " + i));
					text2 = ((All.PayTax.get_PayName(i).Length < 18) ? (text2 + Strings.Space(18 - All.PayTax.get_PayName(i).Length)) : (text2 + " "));
					text2 += All.PayTax.get_PayName(i);
					PayL.Items.Add((object)text2);
				}
			}
		}
	}

	private void LoadTax()
	{
		int taxN = All.PayTax.TaxN;
		checked
		{
			for (int i = 0; i <= taxN; i++)
			{
				if (i == 0)
				{
					string text = "  №     ім'я    %      Акциз";
					TaxL.Items.Add((object)text);
					TaxL.Items.Add((object)"-------------------------------------");
					continue;
				}
				string text2 = ((i.ToString().Length >= 2) ? (Strings.Space(1) + i) : (Strings.Space(2) + i));
				text2 += Strings.Space(7 - All.PayTax.get_TaxName(i).Length);
				text2 += All.PayTax.get_TaxName(i);
				text2 += Strings.Space(7 - All.PayTax.get_TaxPRC(i).Length);
				text2 = text2 + All.PayTax.get_TaxPRC(i) + "%";
				text2 += Strings.Space(8 - All.PayTax.get_TaxEXCISE(i).Length);
				text2 = text2 + All.PayTax.get_TaxEXCISE(i) + "%";
				TaxL.Items.Add((object)text2);
			}
		}
	}
}
