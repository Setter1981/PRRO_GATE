using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
public class FormOfflineStatus : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("ExitB")]
	private Button _ExitB;

	[field: AccessedThroughProperty("ErrT")]
	internal virtual TextBox ErrT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("OnT")]
	internal virtual TextBox OnT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("StartT")]
	internal virtual TextBox StartT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("OstT")]
	internal virtual TextBox OstT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("OstNT")]
	internal virtual TextBox OstNT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button ExitB
	{
		[CompilerGenerated]
		get
		{
			return _ExitB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ExitB_Click;
			Button exitB = _ExitB;
			if (exitB != null)
			{
				((Control)exitB).Click -= eventHandler;
			}
			_ExitB = value;
			exitB = _ExitB;
			if (exitB != null)
			{
				((Control)exitB).Click += eventHandler;
			}
		}
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

	[field: AccessedThroughProperty("Label3")]
	internal virtual Label Label3
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

	[field: AccessedThroughProperty("Label5")]
	internal virtual Label Label5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public FormOfflineStatus()
	{
		((Form)this).Load += FormOfflineStatus_Load;
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
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_0053: Unknown result type (might be due to invalid IL or missing references)
		//IL_005d: Expected O, but got Unknown
		//IL_005e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0068: Expected O, but got Unknown
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		//IL_0073: Expected O, but got Unknown
		//IL_0074: Unknown result type (might be due to invalid IL or missing references)
		//IL_007e: Expected O, but got Unknown
		//IL_007f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0089: Expected O, but got Unknown
		//IL_00a6: Unknown result type (might be due to invalid IL or missing references)
		//IL_00b0: Expected O, but got Unknown
		//IL_0141: Unknown result type (might be due to invalid IL or missing references)
		//IL_014b: Expected O, but got Unknown
		//IL_01d0: Unknown result type (might be due to invalid IL or missing references)
		//IL_01da: Expected O, but got Unknown
		//IL_025f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0269: Expected O, but got Unknown
		//IL_02ee: Unknown result type (might be due to invalid IL or missing references)
		//IL_02f8: Expected O, but got Unknown
		//IL_0380: Unknown result type (might be due to invalid IL or missing references)
		//IL_038a: Expected O, but got Unknown
		//IL_0413: Unknown result type (might be due to invalid IL or missing references)
		//IL_041d: Expected O, but got Unknown
		//IL_0495: Unknown result type (might be due to invalid IL or missing references)
		//IL_049f: Expected O, but got Unknown
		//IL_051a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0524: Expected O, but got Unknown
		//IL_059f: Unknown result type (might be due to invalid IL or missing references)
		//IL_05a9: Expected O, but got Unknown
		//IL_0627: Unknown result type (might be due to invalid IL or missing references)
		//IL_0631: Expected O, but got Unknown
		//IL_078b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0795: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormOfflineStatus));
		ErrT = new TextBox();
		OnT = new TextBox();
		StartT = new TextBox();
		OstT = new TextBox();
		OstNT = new TextBox();
		ExitB = new Button();
		Label2 = new Label();
		Label1 = new Label();
		Label3 = new Label();
		Label4 = new Label();
		Label5 = new Label();
		((Control)this).SuspendLayout();
		((Control)ErrT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)ErrT).Location = new Point(22, 234);
		ErrT.Multiline = true;
		((Control)ErrT).Name = "ErrT";
		((TextBoxBase)ErrT).ReadOnly = true;
		((Control)ErrT).Size = new Size(658, 114);
		((Control)ErrT).TabIndex = 0;
		((Control)ErrT).TabStop = false;
		ErrT.TextAlign = (HorizontalAlignment)2;
		((Control)OnT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OnT).Location = new Point(330, 15);
		((Control)OnT).Name = "OnT";
		((TextBoxBase)OnT).ReadOnly = true;
		((Control)OnT).Size = new Size(350, 30);
		((Control)OnT).TabIndex = 1;
		((Control)OnT).TabStop = false;
		OnT.TextAlign = (HorizontalAlignment)2;
		((Control)StartT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)StartT).Location = new Point(330, 60);
		((Control)StartT).Name = "StartT";
		((TextBoxBase)StartT).ReadOnly = true;
		((Control)StartT).Size = new Size(350, 30);
		((Control)StartT).TabIndex = 2;
		((Control)StartT).TabStop = false;
		StartT.TextAlign = (HorizontalAlignment)2;
		((Control)OstT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OstT).Location = new Point(330, 107);
		((Control)OstT).Name = "OstT";
		((TextBoxBase)OstT).ReadOnly = true;
		((Control)OstT).Size = new Size(350, 30);
		((Control)OstT).TabIndex = 3;
		((Control)OstT).TabStop = false;
		OstT.TextAlign = (HorizontalAlignment)2;
		((Control)OstNT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OstNT).Location = new Point(330, 153);
		((Control)OstNT).Name = "OstNT";
		((TextBoxBase)OstNT).ReadOnly = true;
		((Control)OstNT).Size = new Size(350, 30);
		((Control)OstNT).TabIndex = 4;
		((Control)OstNT).TabStop = false;
		OstNT.TextAlign = (HorizontalAlignment)2;
		((Control)ExitB).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)ExitB).Location = new Point(22, 370);
		((Control)ExitB).Name = "ExitB";
		((Control)ExitB).Size = new Size(658, 38);
		((Control)ExitB).TabIndex = 5;
		((ButtonBase)ExitB).Text = "Закрити ";
		((ButtonBase)ExitB).UseVisualStyleBackColor = true;
		Label2.AutoSize = true;
		((Control)Label2).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label2).Location = new Point(17, 20);
		((Control)Label2).Name = "Label2";
		((Control)Label2).Size = new Size(84, 25);
		((Control)Label2).TabIndex = 10;
		Label2.Text = "Статус:";
		Label1.AutoSize = true;
		((Control)Label1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label1).Location = new Point(17, 65);
		((Control)Label1).Name = "Label1";
		((Control)Label1).Size = new Size(195, 25);
		((Control)Label1).TabIndex = 11;
		Label1.Text = "Початок оффлайн:";
		Label3.AutoSize = true;
		((Control)Label3).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label3).Location = new Point(17, 112);
		((Control)Label3).Name = "Label3";
		((Control)Label3).Size = new Size(263, 25);
		((Control)Label3).TabIndex = 12;
		Label3.Text = "Оффлайн номерів в черзі:";
		Label4.AutoSize = true;
		((Control)Label4).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label4).Location = new Point(17, 158);
		((Control)Label4).Name = "Label4";
		((Control)Label4).Size = new Size(283, 25);
		((Control)Label4).TabIndex = 13;
		Label4.Text = "Залишок резервних номерів:";
		Label5.AutoSize = true;
		((Control)Label5).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label5).Location = new Point(17, 206);
		((Control)Label5).Name = "Label5";
		((Control)Label5).Size = new Size(185, 25);
		((Control)Label5).TabIndex = 14;
		Label5.Text = "Остання помилка:";
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(701, 424);
		((Control)this).Controls.Add((Control)(object)Label5);
		((Control)this).Controls.Add((Control)(object)Label4);
		((Control)this).Controls.Add((Control)(object)Label3);
		((Control)this).Controls.Add((Control)(object)Label1);
		((Control)this).Controls.Add((Control)(object)Label2);
		((Control)this).Controls.Add((Control)(object)ExitB);
		((Control)this).Controls.Add((Control)(object)OstNT);
		((Control)this).Controls.Add((Control)(object)OstT);
		((Control)this).Controls.Add((Control)(object)StartT);
		((Control)this).Controls.Add((Control)(object)OnT);
		((Control)this).Controls.Add((Control)(object)ErrT);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormOfflineStatus";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Статус офлайн режиму ";
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void FormOfflineStatus_Load(object sender, EventArgs e)
	{
		((Form)this).CancelButton = (IButtonControl)(object)ExitB;
		NumbersOfflineUse numbersOfflineUse = new NumbersOfflineUse();
		OstNT.Text = numbersOfflineUse.CountNubmers().ToString();
		ErrT.Text = All.f.StringGetFn(All.A.FN, "LastOfflineErr");
		if (Operators.CompareString(ErrT.Text.Trim(), "", false) == 0)
		{
			ErrT.Text = "Відсутня";
		}
		if (All.A.FullVersion)
		{
			if (All.l.OfflineTrue())
			{
				OnT.Text = "Оффлайн";
				string text = All.l.OfflineDate().ReturnStr.Trim();
				StartT.Text = text;
				OstT.Text = All.l.OfflineCheckCount().ToString();
			}
			else
			{
				OnT.Text = "Онлайн";
				StartT.Text = "---------";
				OstT.Text = "---------";
			}
		}
		else
		{
			OnT.Text = "FREE";
			StartT.Text = "---------";
			OstT.Text = "---------";
			OstNT.Text = "---------";
			ErrT.Text = "---------";
		}
	}

	private void ExitB_Click(object sender, EventArgs e)
	{
		((Form)this).Close();
	}
}
