using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
public class FormCloseShift : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("Bok")]
	private Button _Bok;

	[CompilerGenerated]
	[AccessedThroughProperty("ACS")]
	private CheckBox _ACS;

	internal virtual Button Bok
	{
		[CompilerGenerated]
		get
		{
			return _Bok;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = Bok_Click;
			Button bok = _Bok;
			if (bok != null)
			{
				bok.Click -= value2;
			}
			_Bok = value;
			bok = _Bok;
			if (bok != null)
			{
				bok.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("ComboBox2")]
	internal virtual ComboBox ComboBox2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ComboBox1")]
	internal virtual ComboBox ComboBox1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ComboBox3")]
	internal virtual ComboBox ComboBox3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("GroupBox1")]
	internal virtual GroupBox GroupBox1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual CheckBox ACS
	{
		[CompilerGenerated]
		get
		{
			return _ACS;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ACS_CheckedChanged;
			CheckBox aCS = _ACS;
			if (aCS != null)
			{
				aCS.CheckedChanged -= value2;
			}
			_ACS = value;
			aCS = _ACS;
			if (aCS != null)
			{
				aCS.CheckedChanged += value2;
			}
		}
	}

	[field: AccessedThroughProperty("Label3")]
	internal virtual Label Label3
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

	[field: AccessedThroughProperty("Label4")]
	internal virtual Label Label4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public FormCloseShift()
	{
		base.Load += FormCloseShift_Load;
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
		this.Bok = new System.Windows.Forms.Button();
		this.ComboBox2 = new System.Windows.Forms.ComboBox();
		this.ComboBox1 = new System.Windows.Forms.ComboBox();
		this.ComboBox3 = new System.Windows.Forms.ComboBox();
		this.GroupBox1 = new System.Windows.Forms.GroupBox();
		this.Label3 = new System.Windows.Forms.Label();
		this.Label2 = new System.Windows.Forms.Label();
		this.Label1 = new System.Windows.Forms.Label();
		this.ACS = new System.Windows.Forms.CheckBox();
		this.Label4 = new System.Windows.Forms.Label();
		this.GroupBox1.SuspendLayout();
		base.SuspendLayout();
		this.Bok.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Bok.Location = new System.Drawing.Point(12, 292);
		this.Bok.Name = "Bok";
		this.Bok.Size = new System.Drawing.Size(461, 35);
		this.Bok.TabIndex = 30;
		this.Bok.Text = "Ок";
		this.Bok.UseVisualStyleBackColor = true;
		this.ComboBox2.DropDownStyle = System.Windows.Forms.ComboBoxStyle.DropDownList;
		this.ComboBox2.Font = new System.Drawing.Font("Microsoft Sans Serif", 13.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.ComboBox2.FormattingEnabled = true;
		this.ComboBox2.Location = new System.Drawing.Point(320, 35);
		this.ComboBox2.Name = "ComboBox2";
		this.ComboBox2.Size = new System.Drawing.Size(121, 37);
		this.ComboBox2.TabIndex = 33;
		this.ComboBox1.DropDownStyle = System.Windows.Forms.ComboBoxStyle.DropDownList;
		this.ComboBox1.Font = new System.Drawing.Font("Microsoft Sans Serif", 13.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.ComboBox1.FormattingEnabled = true;
		this.ComboBox1.Location = new System.Drawing.Point(89, 35);
		this.ComboBox1.Name = "ComboBox1";
		this.ComboBox1.Size = new System.Drawing.Size(121, 37);
		this.ComboBox1.TabIndex = 32;
		this.ComboBox3.DropDownStyle = System.Windows.Forms.ComboBoxStyle.DropDownList;
		this.ComboBox3.Font = new System.Drawing.Font("Microsoft Sans Serif", 13.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.ComboBox3.FormattingEnabled = true;
		this.ComboBox3.Location = new System.Drawing.Point(13, 118);
		this.ComboBox3.Name = "ComboBox3";
		this.ComboBox3.Size = new System.Drawing.Size(428, 37);
		this.ComboBox3.TabIndex = 34;
		this.GroupBox1.Controls.Add(this.Label3);
		this.GroupBox1.Controls.Add(this.Label2);
		this.GroupBox1.Controls.Add(this.Label1);
		this.GroupBox1.Controls.Add(this.ComboBox1);
		this.GroupBox1.Controls.Add(this.ComboBox3);
		this.GroupBox1.Controls.Add(this.ComboBox2);
		this.GroupBox1.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.GroupBox1.Location = new System.Drawing.Point(12, 103);
		this.GroupBox1.Name = "GroupBox1";
		this.GroupBox1.Size = new System.Drawing.Size(461, 174);
		this.GroupBox1.TabIndex = 35;
		this.GroupBox1.TabStop = false;
		this.GroupBox1.Text = "Налаштування";
		this.Label3.AutoSize = true;
		this.Label3.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label3.Location = new System.Drawing.Point(8, 90);
		this.Label3.Name = "Label3";
		this.Label3.Size = new System.Drawing.Size(335, 25);
		this.Label3.TabIndex = 38;
		this.Label3.Text = "Провести службову видачу готівки";
		this.Label2.AutoSize = true;
		this.Label2.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label2.Location = new System.Drawing.Point(227, 41);
		this.Label2.Name = "Label2";
		this.Label2.Size = new System.Drawing.Size(87, 25);
		this.Label2.TabIndex = 37;
		this.Label2.Text = "Хвилин:";
		this.Label1.AutoSize = true;
		this.Label1.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label1.Location = new System.Drawing.Point(8, 41);
		this.Label1.Name = "Label1";
		this.Label1.Size = new System.Drawing.Size(75, 25);
		this.Label1.TabIndex = 36;
		this.Label1.Text = "Годин:";
		this.ACS.AutoSize = true;
		this.ACS.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.ACS.Location = new System.Drawing.Point(25, 26);
		this.ACS.Name = "ACS";
		this.ACS.Size = new System.Drawing.Size(130, 29);
		this.ACS.TabIndex = 36;
		this.ACS.Text = "Увімкнути";
		this.ACS.UseVisualStyleBackColor = true;
		this.Label4.Font = new System.Drawing.Font("Microsoft Sans Serif", 9f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label4.Location = new System.Drawing.Point(173, 9);
		this.Label4.Name = "Label4";
		this.Label4.Size = new System.Drawing.Size(300, 91);
		this.Label4.TabIndex = 39;
		this.Label4.Text = "Увага! Закриття зміни можливе лише на ПК з установленим ПРРО. Якщо ПК не працює — зміну закрити неможливо.";
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(488, 339);
		base.Controls.Add(this.Label4);
		base.Controls.Add(this.ACS);
		base.Controls.Add(this.GroupBox1);
		base.Controls.Add(this.Bok);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormCloseShift";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Налаштування автоматичного закриття зміни";
		this.GroupBox1.ResumeLayout(false);
		this.GroupBox1.PerformLayout();
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	private void FormCloseShift_Load(object sender, EventArgs e)
	{
		base.AcceptButton = Bok;
		int num = 0;
		checked
		{
			do
			{
				if (num > 9)
				{
					ComboBox1.Items.Add(num.ToString());
				}
				else
				{
					ComboBox1.Items.Add("0" + num);
				}
				num++;
			}
			while (num <= 23);
			num = 0;
			do
			{
				if (num > 9)
				{
					ComboBox2.Items.Add(num.ToString());
				}
				else
				{
					ComboBox2.Items.Add("0" + num);
				}
				num += 5;
			}
			while (num <= 55);
			ComboBox3.Items.Add("вимкнено");
			ComboBox3.Items.Add("увімкнено");
			TimeIni();
			GroupBox1.Enabled = ACS.Checked;
		}
	}

	private void TimeIni()
	{
		string text = All.f.StringGetFn(All.A.FN, "shiftclosetime");
		if (Versioned.IsNumeric(text))
		{
			TimeSpan timeSpan = TimeSpan.FromMinutes(Conversions.ToDouble(text));
			if (timeSpan.Days > 0)
			{
				ACS.Checked = false;
			}
			else
			{
				ACS.Checked = true;
			}
			ComboBox1.Text = timeSpan.Hours.ToString();
			ComboBox2.Text = timeSpan.Minutes.ToString();
			if (Operators.CompareString(ComboBox1.Text.Trim(), "", TextCompare: false) == 0)
			{
				ComboBox1.Text = "00";
			}
			if (Operators.CompareString(ComboBox2.Text.Trim(), "", TextCompare: false) == 0)
			{
				ComboBox2.Text = "00";
			}
		}
		else
		{
			ACS.Checked = false;
			ComboBox1.Text = "00";
			ComboBox2.Text = "00";
		}
		if (All.f.IntegerGetFn(All.A.FN, "shiftCashInOut") == 1)
		{
			ComboBox3.Text = "увімкнено";
		}
		else
		{
			ComboBox3.Text = "вимкнено";
		}
	}

	private void Bok_Click(object sender, EventArgs e)
	{
		if (Operators.CompareString(ComboBox3.Text, "увімкнено", TextCompare: false) == 0)
		{
			All.f.IntigerWriteFN(All.A.FN, "shiftCashInOut", 1);
		}
		else
		{
			All.f.IntigerWriteFN(All.A.FN, "shiftCashInOut", 0);
		}
		if (Operators.CompareString(ComboBox1.Text.Trim(), "", TextCompare: false) == 0)
		{
			ComboBox1.Text = "00";
		}
		if (Operators.CompareString(ComboBox2.Text.Trim(), "", TextCompare: false) == 0)
		{
			ComboBox2.Text = "00";
		}
		if (ACS.Checked)
		{
			int num = checked(Conversions.ToInteger(ComboBox1.Text.Trim()) * 60 + Conversions.ToInteger(ComboBox2.Text.Trim()));
			All.f.StringWriteFN(All.A.FN, "shiftclosetime", num.ToString());
		}
		else
		{
			All.f.StringWriteFN(All.A.FN, "shiftclosetime", "");
		}
		Close();
	}

	private void ACS_CheckedChanged(object sender, EventArgs e)
	{
		GroupBox1.Enabled = ACS.Checked;
	}
}
