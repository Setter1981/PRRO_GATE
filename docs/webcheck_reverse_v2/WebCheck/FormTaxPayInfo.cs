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
		base.Load += FormTaxPayInfo_Load;
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
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormTaxPayInfo));
		this.TaxL = new System.Windows.Forms.ListBox();
		this.PayL = new System.Windows.Forms.ListBox();
		this.Label2 = new System.Windows.Forms.Label();
		this.Label1 = new System.Windows.Forms.Label();
		base.SuspendLayout();
		this.TaxL.Font = new System.Drawing.Font("Consolas", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TaxL.FormattingEnabled = true;
		this.TaxL.ItemHeight = 23;
		this.TaxL.Location = new System.Drawing.Point(12, 38);
		this.TaxL.Name = "TaxL";
		this.TaxL.Size = new System.Drawing.Size(383, 372);
		this.TaxL.TabIndex = 0;
		this.PayL.Font = new System.Drawing.Font("Consolas", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.PayL.FormattingEnabled = true;
		this.PayL.ItemHeight = 23;
		this.PayL.Location = new System.Drawing.Point(415, 38);
		this.PayL.Name = "PayL";
		this.PayL.Size = new System.Drawing.Size(336, 372);
		this.PayL.TabIndex = 1;
		this.Label2.AutoSize = true;
		this.Label2.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label2.Location = new System.Drawing.Point(12, 10);
		this.Label2.Name = "Label2";
		this.Label2.Size = new System.Drawing.Size(243, 25);
		this.Label2.TabIndex = 2;
		this.Label2.Text = "Інформація про податки";
		this.Label1.AutoSize = true;
		this.Label1.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label1.Location = new System.Drawing.Point(410, 10);
		this.Label1.Name = "Label1";
		this.Label1.Size = new System.Drawing.Size(243, 25);
		this.Label1.TabIndex = 3;
		this.Label1.Text = "Інформація про платежі";
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(765, 420);
		base.Controls.Add(this.Label1);
		base.Controls.Add(this.Label2);
		base.Controls.Add(this.PayL);
		base.Controls.Add(this.TaxL);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormTaxPayInfo";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterParent;
		this.Text = "Довідкова інформація";
		base.ResumeLayout(false);
		base.PerformLayout();
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
					string item = "  №      ім'я платежу";
					PayL.Items.Add(item);
					PayL.Items.Add("-----------------------------");
				}
				else
				{
					string text = ((i.ToString().Length >= 2) ? (Strings.Space(1) + i) : (Strings.Space(1) + " " + i));
					text = ((All.PayTax.get_PayName(i).Length < 18) ? (text + Strings.Space(18 - All.PayTax.get_PayName(i).Length)) : (text + " "));
					text += All.PayTax.get_PayName(i);
					PayL.Items.Add(text);
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
					string item = "  №     ім'я    %      Акциз";
					TaxL.Items.Add(item);
					TaxL.Items.Add("-------------------------------------");
					continue;
				}
				string text = ((i.ToString().Length >= 2) ? (Strings.Space(1) + i) : (Strings.Space(2) + i));
				text += Strings.Space(7 - All.PayTax.get_TaxName(i).Length);
				text += All.PayTax.get_TaxName(i);
				text += Strings.Space(7 - All.PayTax.get_TaxPRC(i).Length);
				text = text + All.PayTax.get_TaxPRC(i) + "%";
				text += Strings.Space(8 - All.PayTax.get_TaxEXCISE(i).Length);
				text = text + All.PayTax.get_TaxEXCISE(i) + "%";
				TaxL.Items.Add(text);
			}
		}
	}
}
